//! Settings, hub account, publishing, servers and deploys.

use crate::util::{Res, blocking, open, progress};
use anyhow::{Context, anyhow};
use beacon_core::{
    api::{AllServers, ConsoleView, Credentials, DeployRequest, DeployResponse, DeployTargets, Release, Server, ServerAction},
    hub::Hub,
    instance, ops, server,
    settings::Settings,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;

#[derive(Serialize)]
pub struct SettingsView {
    /// Hub in use (custom or default).
    hub: String,
    hub_custom: bool,
    username: Option<String>,
    signed_in: bool,
    curseforge_key: String,
    /// Folder new instances go to, and whether the user picked it.
    instances_dir: PathBuf,
    instances_custom: bool,
    prism_path: Option<PathBuf>,
    prism_found: bool,
}

#[tauri::command]
pub async fn get_settings() -> Res<SettingsView> {
    let s = Settings::load();
    Ok(SettingsView {
        hub: s.hub_url(),
        hub_custom: s.hub.is_some(),
        username: s.username.clone(),
        signed_in: s.token.is_some(),
        curseforge_key: s.curseforge_key.clone().unwrap_or_default(),
        instances_dir: s.instances_root(),
        instances_custom: s.instances_dir.is_some(),
        prism_found: instance::prism_command(s.prism_path.as_deref()).is_some(),
        prism_path: s.prism_path,
    })
}

/// Editable settings. Empty strings mean "use the default".
#[derive(Deserialize)]
pub struct SettingsForm {
    hub: String,
    curseforge_key: String,
    instances_dir: String,
    prism_path: String,
}

#[tauri::command]
pub async fn save_settings(form: SettingsForm) -> Res<()> {
    let mut s = Settings::load();
    let opt = |v: String| Some(v.trim().to_string()).filter(|v| !v.is_empty());
    let hub = opt(form.hub).map(|h| h.trim_end_matches('/').to_string());
    if hub != s.hub {
        // Tokens belong to one hub.
        s.token = None;
        s.username = None;
    }
    s.hub = hub;
    s.curseforge_key = opt(form.curseforge_key);
    s.instances_dir = opt(form.instances_dir).map(PathBuf::from);
    s.prism_path = opt(form.prism_path).map(PathBuf::from);
    s.save()?;
    Ok(())
}

/// Signs in (or creates an account when `create`) and stores the token.
#[tauri::command]
pub async fn sign_in(username: String, password: String, create: bool) -> Res<String> {
    let mut s = Settings::load();
    let hub = Hub::from_settings(&s);
    let c = Credentials { username: username.trim().to_string(), password };
    let auth = if create { hub.register(&c).await? } else { hub.login(&c).await? };
    s.token = Some(auth.token);
    s.username = Some(auth.user.username.clone());
    s.save()?;
    Ok(auth.user.username)
}

#[tauri::command]
pub async fn sign_out() -> Res<()> {
    let mut s = Settings::load();
    s.token = None;
    s.username = None;
    s.save()?;
    Ok(())
}

/// Approves the code shown by `beacon login` on another machine.
#[tauri::command]
pub async fn approve_device(code: String) -> Res<()> {
    Hub::authed(&Settings::load())?.device_approve(code.trim()).await?;
    Ok(())
}

fn hub_and_pack(path: &str) -> anyhow::Result<(Hub, String)> {
    let remote = open(path)?.config()?.remote.context("publish this pack first")?;
    Ok((Hub::authed(&Settings::load())?, remote.pack))
}

#[tauri::command]
pub async fn publish(path: String, name: String, notes: String) -> Res<Release> {
    let repo = open(&path)?;
    let hub = Hub::authed(&Settings::load())?;
    if name.trim().is_empty() {
        return Err(anyhow!("give this update a name").into());
    }
    Ok(ops::publish(&repo, &hub, name.trim(), notes.trim()).await?)
}

#[tauri::command]
pub async fn releases(path: String) -> Res<Vec<Release>> {
    let (hub, pack) = hub_and_pack(&path)?;
    Ok(hub.releases(&pack).await?)
}

#[tauri::command]
pub async fn servers(path: String) -> Res<Vec<Server>> {
    let (hub, pack) = hub_and_pack(&path)?;
    Ok(hub.servers(&pack).await?)
}

/// A server's console after `after` (its last lines when null), waiting up to 20s for new ones.
#[tauri::command]
pub async fn server_console(path: String, server: String, after: Option<u64>) -> Res<ConsoleView> {
    let (hub, _) = hub_and_pack(&path)?;
    Ok(hub.console(&server, after, if after.is_some() { 20 } else { 0 }).await?)
}

/// Sends a console command, start/stop/restart or a backup action to a server.
#[tauri::command]
pub async fn server_action(path: String, server: String, action: ServerAction) -> Res<()> {
    let (hub, _) = hub_and_pack(&path)?;
    hub.server_action(&server, &action).await?;
    Ok(())
}

/// Deploys a release to the given servers, or to all of them when `servers` is null.
#[tauri::command]
pub async fn deploy(path: String, release: String, servers: Option<Vec<String>>) -> Res<DeployResponse> {
    let (hub, pack) = hub_and_pack(&path)?;
    let servers = servers.map_or(DeployTargets::All(AllServers::All), DeployTargets::Some);
    Ok(hub.deploy(&pack, &DeployRequest { release, servers }).await?)
}

/// Builds a ready to run server folder from the last saved version and zips it to `out`.
#[tauri::command]
pub async fn export_server_pack(app: AppHandle, path: String, out: PathBuf) -> Res<()> {
    let repo = open(&path)?;
    let tree = repo.head_tree()?.context("save a version first")?;
    let name = repo.config()?.name;
    let tmp = std::env::temp_dir().join(format!("beaconsync-server-{}", beacon_core::now()));
    let built = server::build_folder(&tmp, &name, &tree, &repo.fetcher(), 4096, &progress(&app)).await;
    let result = match built {
        Ok(_) => {
            let dir = tmp.clone();
            blocking(move || server::zip_dir(&dir, &out)).await
        }
        Err(e) => Err(e.into()),
    };
    std::fs::remove_dir_all(&tmp).ok();
    result
}
