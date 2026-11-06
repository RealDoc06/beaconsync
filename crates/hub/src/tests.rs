//! End to end hub flow over HTTP: account, pack, push, ref conflicts, release, deploy and the
//! agent long poll.

use crate::{Config, routes};
use beacon_core::{
    api::*,
    hub::Hub,
    model::{Commit, Loader, Tree},
};
use std::time::Duration;

async fn start() -> (tempfile::TempDir, String) {
    let tmp = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let config = Config { addr, data: tmp.path().into(), public_url: None, open_registration: true, max_object_mb: 8 };
    let app = routes::router(routes::AppState::open(config).unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (tmp, format!("http://{addr}"))
}

/// Uploads a one-file tree + commit and returns the commit id.
async fn upload_commit(hub: &Hub, parents: Vec<String>, msg: &str) -> String {
    let tree = serde_json::to_vec(&Tree::new("1.21.1", Loader::default())).unwrap();
    let tree_id = beacon_core::hash::sha512(&tree);
    hub.put_object(&tree_id, tree).await.unwrap();
    let commit = Commit { format: 1, tree: tree_id, parents, author: "a".into(), message: msg.into(), time: 0 };
    let bytes = serde_json::to_vec(&commit).unwrap();
    let id = beacon_core::hash::sha512(&bytes);
    hub.put_object(&id, bytes).await.unwrap();
    id
}

#[tokio::test]
async fn publish_and_deploy_flow() {
    let (_tmp, base) = start().await;
    let anon = Hub::new(&base, None);
    let auth = anon.register(&Credentials { username: "doc".into(), password: "longpassword".into() }).await.unwrap();
    let hub = Hub::new(&base, Some(auth.token));

    let pack = hub
        .create_pack(&NewPack {
            name: "Pack".into(),
            description: String::new(),
            minecraft: "1.21.1".into(),
            loader: Loader::default(),
        })
        .await
        .unwrap();
    let c1 = upload_commit(&hub, vec![], "one").await;
    assert!(hub.missing(vec![c1.clone()]).await.unwrap().is_empty());
    hub.update_ref(&pack.id, "main", &RefUpdate { old: None, new: c1.clone(), force: false }).await.unwrap();

    // A stale `old` is rejected so pushes never silently drop someone else's versions.
    let c2 = upload_commit(&hub, vec![c1.clone()], "two").await;
    assert!(hub.update_ref(&pack.id, "main", &RefUpdate { old: None, new: c2.clone(), force: false }).await.is_err());
    hub.update_ref(&pack.id, "main", &RefUpdate { old: Some(c1), new: c2.clone(), force: false }).await.unwrap();

    let release =
        hub.create_release(&pack.id, &NewRelease { commit: c2.clone(), name: "v1".into(), notes: String::new() }).await.unwrap();
    let public = anon.public_pack(&pack.code.to_lowercase()).await.unwrap();
    assert_eq!(public.latest.unwrap().commit, c2);

    let server = hub
        .create_server(&pack.id, &NewServer { name: "s1".into(), mode: ServerMode::Service, address: "mc:25565".into() })
        .await
        .unwrap();
    let agent = Hub::new(&base, Some(server.token));
    assert!(agent.agent_poll(0, None).await.unwrap().target.is_none());
    assert!(agent.library().await.is_err(), "server tokens can't act as users");

    // A waiting agent is woken by the deploy instead of waiting out its poll.
    let waiting = tokio::spawn(async move {
        let started = std::time::Instant::now();
        let poll = agent.agent_poll(20, None).await.unwrap();
        (poll, started.elapsed(), agent)
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    hub.deploy(&pack.id, &DeployRequest { release: release.id, servers: DeployTargets::All(AllServers::All) }).await.unwrap();
    let (poll, elapsed, agent) = waiting.await.unwrap();
    assert!(elapsed < Duration::from_secs(5));
    let target = poll.target.unwrap();
    assert_eq!(target.commit, c2);

    agent
        .agent_report(&AgentReport {
            deployment: Some(target.deployment),
            status: "done".into(),
            message: "ok".into(),
            commit: Some(c2.clone()),
            agent: None,
        })
        .await
        .unwrap();
    assert!(agent.agent_poll(0, None).await.unwrap().target.is_none());
    let servers = hub.servers(&pack.id).await.unwrap();
    assert_eq!(servers[0].current_commit.as_deref(), Some(c2.as_str()));
    assert!(servers[0].online);
}

#[tokio::test]
async fn device_login() {
    let (_tmp, base) = start().await;
    let anon = Hub::new(&base, None);
    let user = anon.register(&Credentials { username: "doc".into(), password: "longpassword".into() }).await.unwrap();
    let start = anon.device_start().await.unwrap();
    assert!(anon.device_poll(&start.device_code).await.unwrap().is_none());
    Hub::new(&base, Some(user.token)).device_approve(&start.user_code.to_lowercase()).await.unwrap();
    let got = anon.device_poll(&start.device_code).await.unwrap().unwrap();
    assert_eq!(got.user.username, "doc");
    assert!(anon.device_poll(&start.device_code).await.is_err(), "codes are single use");
}

#[tokio::test]
async fn rolled_back_deploys_are_not_resent() {
    let (_tmp, base) = start().await;
    let auth =
        Hub::new(&base, None).register(&Credentials { username: "doc".into(), password: "longpassword".into() }).await.unwrap();
    let hub = Hub::new(&base, Some(auth.token));
    let pack = hub
        .create_pack(&NewPack {
            name: "Pack".into(),
            description: String::new(),
            minecraft: "1.21.1".into(),
            loader: Loader::default(),
        })
        .await
        .unwrap();
    let c1 = upload_commit(&hub, vec![], "one").await;
    let r1 =
        hub.create_release(&pack.id, &NewRelease { commit: c1.clone(), name: "v1".into(), notes: String::new() }).await.unwrap();
    let s = hub
        .create_server(&pack.id, &NewServer { name: "s".into(), mode: ServerMode::Service, address: "mc".into() })
        .await
        .unwrap();
    let agent = Hub::new(&base, Some(s.token));
    let deploy = |release: String| DeployRequest { release, servers: DeployTargets::All(AllServers::All) };
    let report =
        |status: &str| AgentReport { deployment: None, status: status.into(), message: String::new(), commit: None, agent: None };

    hub.deploy(&pack.id, &deploy(r1.id.clone())).await.unwrap();
    assert!(agent.agent_poll(0, None).await.unwrap().target.is_some());
    agent.agent_report(&report("rolledback")).await.unwrap();
    assert!(agent.agent_poll(0, None).await.unwrap().target.is_none());
    // Restarting the supervisor reports "running"; the rolled back deploy must stay put.
    agent.agent_report(&report("running")).await.unwrap();
    assert!(agent.agent_poll(0, None).await.unwrap().target.is_none());
    // A new deploy is offered again.
    hub.deploy(&pack.id, &deploy(r1.id)).await.unwrap();
    assert!(agent.agent_poll(0, None).await.unwrap().target.is_some());
}

#[tokio::test]
async fn rollback_publishes_old_files_as_a_new_release_and_deploys_it() {
    use beacon_core::{fetch::no_progress, instance, model::LoaderKind, ops, repo};
    let (tmp, base) = start().await;
    let auth =
        Hub::new(&base, None).register(&Credentials { username: "doc".into(), password: "longpassword".into() }).await.unwrap();
    let hub = Hub::new(&base, Some(auth.token));

    let dir = tmp.path().join("pack");
    instance::create(&dir, "Pack", "1.21.1", &Loader { kind: LoaderKind::Fabric, version: Some("0.16.9".into()) }).unwrap();
    let repo = repo::Repo::init(&dir, repo::local_config("Pack")).unwrap();
    let cfg = repo.game_dir().join("config/a.toml");
    std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    std::fs::write(&cfg, "1").unwrap();
    let v1 = repo.commit("v1", "doc").await.unwrap();
    ops::publish(&repo, &hub, "1.0", "").await.unwrap();
    std::fs::write(&cfg, "2").unwrap();
    repo.commit("v2", "doc").await.unwrap();
    ops::publish(&repo, &hub, "1.1", "").await.unwrap();
    let pack = repo.config().unwrap().remote.unwrap().pack;
    hub.create_server(&pack, &NewServer { name: "s1".into(), mode: ServerMode::Service, address: "mc:25565".into() })
        .await
        .unwrap();

    let r = ops::rollback(&repo, Some(&hub), &v1, "doc", &no_progress).await.unwrap();
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "1");
    assert_eq!(repo.commit_tree(&r.commit).unwrap(), repo.commit_tree(&v1).unwrap());
    assert_eq!(repo.log("HEAD", 10).unwrap().len(), 3, "history only moves forward");
    assert_eq!(r.release.unwrap().name, "1.0 (rollback)");
    assert_eq!(r.servers, 1);
    assert_eq!(hub.servers(&pack).await.unwrap()[0].target_commit.as_deref(), Some(r.commit.as_str()));
}

#[tokio::test]
async fn console_and_actions_are_relayed_between_owner_and_server() {
    let (_tmp, base) = start().await;
    let anon = Hub::new(&base, None);
    let owner = Hub::new(
        &base,
        Some(anon.register(&Credentials { username: "doc".into(), password: "longpassword".into() }).await.unwrap().token),
    );
    let other = Hub::new(
        &base,
        Some(anon.register(&Credentials { username: "eve".into(), password: "longpassword".into() }).await.unwrap().token),
    );
    let pack = owner
        .create_pack(&NewPack {
            name: "P".into(),
            description: String::new(),
            minecraft: "1.21.1".into(),
            loader: Loader::default(),
        })
        .await
        .unwrap();
    let created = owner
        .create_server(&pack.id, &NewServer { name: "S".into(), mode: ServerMode::Service, address: "x:25565".into() })
        .await
        .unwrap();
    let (sid, agent) = (created.server.id.clone(), Hub::new(&base, Some(created.token)));
    agent.agent_poll(0, None).await.unwrap();

    let backup = Backup {
        id: "backups/a.zip".into(),
        source: "FTB Backups".into(),
        note: "a.zip".into(),
        size: 1,
        created: 0,
        full: false,
    };
    agent
        .agent_console(&AgentConsole {
            lines: vec!["Done (3.2s)!".into()],
            backups: Some(vec![backup.clone()]),
            running: Some(true),
        })
        .await
        .unwrap();
    let v = owner.console(&sid, None, 0).await.unwrap();
    assert_eq!((v.lines, v.backups, v.running, v.next), (vec!["Done (3.2s)!".to_string()], vec![backup], true, 1));

    // A viewer waiting after the cursor is woken by the next line.
    let waiting = tokio::spawn({
        let owner = owner.clone();
        let sid = sid.clone();
        async move { owner.console(&sid, Some(1), 10).await.unwrap() }
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    agent.agent_console(&AgentConsole { lines: vec!["Steve joined the game".into()], ..Default::default() }).await.unwrap();
    let v = tokio::time::timeout(Duration::from_secs(3), waiting).await.unwrap().unwrap();
    assert_eq!(v.lines, vec!["Steve joined the game".to_string()]);

    // Actions reach a long polling agent right away, once.
    let polling = tokio::spawn({
        let agent = agent.clone();
        async move { agent.agent_poll(20, None).await.unwrap() }
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    owner.server_action(&sid, &ServerAction::Command("list".into())).await.unwrap();
    let p = tokio::time::timeout(Duration::from_secs(3), polling).await.unwrap().unwrap();
    assert_eq!(p.actions, vec![ServerAction::Command("list".into())]);
    assert!(agent.agent_poll(0, None).await.unwrap().actions.is_empty());

    // Only the owner can watch or control the server.
    assert!(other.console(&sid, None, 0).await.is_err());
    assert!(other.server_action(&sid, &ServerAction::Stop).await.is_err());
}

#[tokio::test]
async fn update_is_only_relayed_to_agents_that_report_a_version() {
    let (_tmp, base) = start().await;
    let anon = Hub::new(&base, None);
    let owner = Hub::new(
        &base,
        Some(anon.register(&Credentials { username: "doc".into(), password: "longpassword".into() }).await.unwrap().token),
    );
    let pack = owner
        .create_pack(&NewPack {
            name: "P".into(),
            description: String::new(),
            minecraft: "1.21.1".into(),
            loader: Loader::default(),
        })
        .await
        .unwrap();
    let created = owner
        .create_server(&pack.id, &NewServer { name: "S".into(), mode: ServerMode::Service, address: "x:25565".into() })
        .await
        .unwrap();
    let (sid, agent) = (created.server.id.clone(), Hub::new(&base, Some(created.token)));
    let update = ServerAction::Update("0.2.0".into());
    let report = |agent_version: Option<&str>| AgentReport {
        deployment: None,
        status: "running".into(),
        message: String::new(),
        commit: None,
        agent: agent_version.map(Into::into),
    };

    // An old agent can't parse `update`, so the hub refuses it.
    agent.agent_report(&report(None)).await.unwrap();
    assert!(owner.server_action(&sid, &update).await.is_err());

    agent.agent_report(&report(Some("0.1.0"))).await.unwrap();
    assert_eq!(owner.servers(&pack.id).await.unwrap()[0].agent_version.as_deref(), Some("0.1.0"));
    owner.server_action(&sid, &update).await.unwrap();
    assert_eq!(agent.agent_poll(0, None).await.unwrap().actions, vec![update]);
}
