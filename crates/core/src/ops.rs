//! High level workflows that combine the repo with a hub: publish, push, join, fork,
//! following releases and pulling upstream changes.

use crate::{
    api::{NewPack, NewRelease, PackPatch, RefUpdate, Release},
    fetch::OnProgress,
    hub::Hub,
    import, instance,
    merge::MergeResult,
    model::{Commit, Tree},
    repo::{PackConfig, Remote, Repo, Role, Upstream},
    sources::modrinth,
};
use anyhow::{Context, Result, bail};
use futures::{StreamExt, TryStreamExt, stream};
use serde::Serialize;
use std::{
    collections::{HashSet, VecDeque},
    path::Path,
};

/// Downloads a commit and its history (commits + trees, not blobs) into the local store.
pub async fn fetch_commit(repo: &Repo, hub: &Hub, commit: &str) -> Result<()> {
    let mut queue = VecDeque::from([commit.to_string()]);
    let mut seen = HashSet::new();
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if !repo.store.has(&id) {
            repo.store.put_verified(&id, &hub.get_object(&id).await?)?;
        } else if repo.read_commit(&id).is_ok_and(|c| repo.store.has(&c.tree)) {
            continue; // already have this part of history
        }
        let c: Commit = repo.read_commit(&id)?;
        if !repo.store.has(&c.tree) {
            repo.store.put_verified(&c.tree, &hub.get_object(&c.tree).await?)?;
        }
        queue.extend(c.parents);
    }
    Ok(())
}

/// Uploads everything reachable from the branch tip that the hub lacks, then moves the
/// hub's branch. Refuses to overwrite newer remote history unless `force`.
pub async fn push(repo: &Repo, hub: &Hub, force: bool) -> Result<String> {
    let cfg = repo.config()?;
    let remote = cfg.remote.context("this pack is not published yet")?;
    let local = repo.branch_commit(&remote.branch)?.context("nothing saved on this branch yet")?;
    let remote_tip = hub.refs(&remote.pack).await?.get(&remote.branch).cloned();
    if remote_tip.as_deref() == Some(local.as_str()) {
        return Ok(local);
    }
    if let Some(r) = &remote_tip
        && !force
        && !repo.is_ancestor(r, &local)
    {
        bail!("the hub has versions you don't have yet; pull first");
    }
    let known: HashSet<String> = match &remote_tip {
        Some(r) if repo.store.has(r) => repo.log(r, usize::MAX)?.into_iter().map(|e| e.id).collect(),
        _ => HashSet::new(),
    };
    let commits: Vec<String> = repo.log(&local, usize::MAX)?.into_iter().map(|e| e.id).filter(|id| !known.contains(id)).collect();
    let missing = hub.missing(repo.objects_for(&commits)?).await?;
    stream::iter(missing)
        .map(|h| async move { hub.put_object(&h, repo.store.get(&h)?).await })
        .buffer_unordered(4)
        .try_collect::<()>()
        .await?;
    hub.update_ref(&remote.pack, &remote.branch, &RefUpdate { old: remote_tip, new: local.clone(), force }).await?;
    Ok(local)
}

/// Creates the pack on the hub the first time it is published.
pub async fn ensure_remote(repo: &Repo, hub: &Hub) -> Result<Remote> {
    let mut cfg = repo.config()?;
    if let Some(r) = &cfg.remote
        && cfg.role == Role::Owner
    {
        return Ok(r.clone());
    }
    let tree = repo.head_tree()?.context("save a version before publishing")?;
    let pack = hub
        .create_pack(&NewPack {
            name: cfg.name.clone(),
            description: String::new(),
            minecraft: tree.minecraft.clone(),
            loader: tree.loader.clone(),
        })
        .await?;
    let remote = Remote { hub: hub.base.clone(), pack: pack.id, code: pack.code, branch: "main".into() };
    cfg.remote = Some(remote.clone());
    cfg.role = Role::Owner;
    repo.save_config(&cfg)?;
    Ok(remote)
}

/// Pushes the current branch and creates a release players and servers can follow.
pub async fn publish(repo: &Repo, hub: &Hub, name: &str, notes: &str) -> Result<Release> {
    if repo.is_dirty()? {
        bail!("save your changes as a version before publishing");
    }
    let mut remote = ensure_remote(repo, hub).await?;
    if let crate::repo::Head::Branch(b) = repo.head()?
        && b != remote.branch
    {
        // Publishing from another branch publishes that branch.
        remote.branch = b;
        let mut cfg = repo.config()?;
        cfg.remote = Some(remote.clone());
        repo.save_config(&cfg)?;
    }
    let commit = push(repo, hub, false).await?;
    let tree = repo.commit_tree(&commit)?;
    hub.patch_pack(&remote.pack, &PackPatch { minecraft: Some(tree.minecraft), loader: Some(tree.loader), ..Default::default() })
        .await?;
    hub.create_release(&remote.pack, &NewRelease { commit, name: name.into(), notes: notes.into() }).await
}

/// Clones a published pack by share code into a new instance.
/// `fork = false` follows the owner's releases (subscriber); `fork = true` makes an
/// editable copy that can pull upstream changes later.
pub async fn join(hub: &Hub, code: &str, root: &Path, fork: bool, author: &str, on: OnProgress<'_>) -> Result<Repo> {
    let info = hub.public_pack(code).await?;
    let latest = info.latest.clone().context("this pack has no published version yet")?;
    let name = if fork { format!("{} (fork)", info.name) } else { info.name.clone() };
    let dir = instance::unique_dir(root, &name);
    let tmp = Repo::init(&dir, crate::repo::local_config(&name))?;
    fetch_commit(&tmp, hub, &latest.commit).await?;
    let tree: Tree = tmp.commit_tree(&latest.commit)?;
    instance::create(&dir, &name, &tree.minecraft, &tree.loader)?;
    let config = if fork {
        PackConfig {
            name: name.clone(),
            upstream: Some(Upstream::Beacon {
                hub: hub.base.clone(),
                code: info.code.clone(),
                name: info.name.clone(),
                commit: latest.commit.clone(),
                base_tree: tmp.read_commit(&latest.commit)?.tree,
            }),
            ..crate::repo::local_config(&name)
        }
    } else {
        PackConfig {
            name: name.clone(),
            role: Role::Subscriber,
            remote: Some(Remote { hub: hub.base.clone(), pack: info.id.clone(), code: info.code.clone(), branch: "main".into() }),
            ..crate::repo::local_config(&name)
        }
    };
    let repo = Repo::init(&dir, config)?;
    repo.checkout_tree(&tree, None, on).await?;
    if let Err(e) = crate::companion::install(&repo).await {
        tracing::warn!("companion mod not installed: {e:#}");
    }
    if fork {
        repo.commit(&format!("Forked {} {}", info.name, latest.name), author).await?;
    } else {
        repo.set_branch("main", &latest.commit)?;
        if hub.me().await.is_ok() {
            hub.join(&info.code).await.ok();
        }
    }
    Ok(repo)
}

#[derive(Serialize, Clone, Debug)]
pub struct UpdateInfo {
    pub available: bool,
    pub name: String,
    pub commit: Option<String>,
    pub current: Option<String>,
}

/// For subscribed packs: is there a newer release than what is installed?
pub async fn check_release(repo: &Repo) -> Result<UpdateInfo> {
    let cfg = repo.config()?;
    let remote = cfg.remote.context("not linked to a hub")?;
    let info = Hub::new(&remote.hub, None).public_pack(&remote.code).await?;
    let current = repo.head_commit()?;
    let latest = info.latest;
    Ok(UpdateInfo {
        available: latest.as_ref().is_some_and(|l| Some(&l.commit) != current.as_ref()),
        name: latest.as_ref().map(|l| l.name.clone()).unwrap_or_default(),
        commit: latest.map(|l| l.commit),
        current,
    })
}

/// Installs the newest release of a subscribed pack (local edits are replaced).
pub async fn update_subscribed(repo: &Repo, on: OnProgress<'_>) -> Result<UpdateInfo> {
    let info = check_release(repo).await?;
    let Some(commit) = info.commit.clone().filter(|_| info.available) else { return Ok(info) };
    let remote = repo.config()?.remote.context("not linked")?;
    let hub = Hub::new(&remote.hub, None);
    fetch_commit(repo, &hub, &commit).await?;
    let tree = repo.commit_tree(&commit)?;
    repo.checkout_tree(&tree, None, on).await?;
    repo.set_branch(&remote.branch, &commit)?;
    std::fs::write(repo.root.join(".beacon/HEAD"), format!("ref: refs/heads/{}\n", remote.branch))?;
    Ok(UpdateInfo { available: false, ..info })
}

/// For owners editing on several machines: brings in hub versions newer than local ones.
pub async fn pull(repo: &Repo, hub: &Hub, on: OnProgress<'_>) -> Result<Option<MergeResult>> {
    let remote = repo.config()?.remote.context("not published")?;
    let Some(tip) = hub.refs(&remote.pack).await?.get(&remote.branch).cloned() else { return Ok(None) };
    fetch_commit(repo, hub, &tip).await?;
    let local = repo.branch_commit(&remote.branch)?;
    match local {
        None => {
            repo.set_branch(&remote.branch, &tip)?;
            repo.checkout_tree(&repo.commit_tree(&tip)?, None, on).await?;
            Ok(None)
        }
        Some(l) if l == tip || repo.is_ancestor(&tip, &l) => Ok(None),
        Some(_) => Ok(Some(repo.merge(&tip, on).await?)),
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct UpstreamInfo {
    pub available: bool,
    pub name: String,
    pub version: String,
}

/// Checks whether the pack this one was forked/imported from has a newer version.
pub async fn check_upstream(repo: &Repo) -> Result<UpstreamInfo> {
    // A merged but unsaved upstream pull counts as synced.
    let upstream = repo.pending_upstream().or(repo.config()?.upstream).context("this pack has no upstream")?;
    match upstream {
        Upstream::Beacon { hub, code, name, commit, .. } => {
            let info = Hub::new(&hub, None).public_pack(&code).await?;
            let latest = info.latest.context("upstream has no releases")?;
            Ok(UpstreamInfo { available: latest.commit != commit, name, version: latest.name })
        }
        Upstream::Modrinth { project, name, version, .. } => {
            let latest = modrinth::versions(&project, None, None).await?.into_iter().next().context("no versions")?;
            Ok(UpstreamInfo { available: latest.id != version, name, version: latest.version_number })
        }
    }
}

/// Merges the newest upstream version into the working tree. Your own changes are kept;
/// files both sides changed are returned as conflicts.
pub async fn pull_upstream(repo: &Repo, on: OnProgress<'_>) -> Result<MergeResult> {
    if repo.is_dirty()? {
        bail!("save or discard your changes before pulling upstream");
    }
    match repo.config()?.upstream.context("this pack has no upstream")? {
        Upstream::Beacon { hub, code, name, .. } => {
            let h = Hub::new(&hub, None);
            let latest = h.public_pack(&code).await?.latest.context("upstream has no releases")?;
            fetch_commit(repo, &h, &latest.commit).await?;
            let tree_id = repo.read_commit(&latest.commit)?.tree;
            let theirs = repo.read_tree(&tree_id)?;
            let next = Upstream::Beacon { hub, code, name, commit: latest.commit, base_tree: tree_id };
            repo.merge_upstream(&theirs, next, on).await
        }
        Upstream::Modrinth { project, name, .. } => {
            let v = modrinth::versions(&project, None, None).await?.into_iter().next().context("no versions")?;
            let file = v.primary_file().context("no file")?;
            let tmp = instance::data_dir().join("downloads").join(&file.filename);
            crate::fetch::download(&file.url, &file.hashes.sha512, &tmp).await?;
            let (_, theirs) = import::mrpack_tree(&tmp, &repo.store).await?;
            std::fs::remove_file(&tmp).ok();
            let base_tree = repo.store_tree(&theirs)?;
            let next = Upstream::Modrinth { project, name, version: v.id, base_tree };
            repo.merge_upstream(&theirs, next, on).await
        }
    }
}

/// Address of an online server running this pack, so "Play" can drop players straight in.
pub async fn server_address(repo: &Repo) -> Option<String> {
    let remote = repo.config().ok()?.remote?;
    let info = Hub::new(&remote.hub, None).public_pack(&remote.code).await.ok()?;
    info.servers.into_iter().find(|s| s.online).map(|s| s.address)
}
