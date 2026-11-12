//! Commands for one pack: files, mods, changes, history, branches, upstream and
//! subscribed updates, plus header actions (play, rename, delete, export).

use crate::util::{Res, author, blocking, open, progress};
use anyhow::{Context, anyhow};
use beacon_core::{
    fetch::Fetcher,
    import, instance,
    instance::InstanceInfo,
    merge::{Conflict, Pick},
    model::{Entry, LoaderKind, Side, Source, Tree, enabled_path, is_disabled, is_mod_path},
    ops::{self, UpdateInfo, UpstreamInfo},
    prism,
    repo::{BranchInfo, LogEntry, PackConfig, Repo, Status},
    settings::Settings,
    sources::modrinth::{self, ProjectType, Update},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Emitter};

#[derive(Serialize)]
pub struct PackInfo {
    #[serde(flatten)]
    info: InstanceInfo,
    config: PackConfig,
    branch: Option<String>,
    /// `{hub}/j/{code}` once published.
    share_link: Option<String>,
}

#[tauri::command]
pub async fn pack_info(path: String) -> Res<PackInfo> {
    let repo = open(&path)?;
    let config = repo.config()?;
    Ok(PackInfo {
        info: instance::info(&repo.root)?,
        branch: match repo.head()? {
            beacon_core::repo::Head::Branch(b) => Some(b),
            beacon_core::repo::Head::Detached(_) => None,
        },
        share_link: config.remote.as_ref().map(|r| format!("{}/j/{}", r.hub, r.code)),
        config,
    })
}

/// Folders shown in the Mods tab.
const CONTENT_FOLDERS: &[&str] = &["mods", "resourcepacks", "shaderpacks"];

#[derive(Serialize)]
pub struct FileRow {
    path: String,
    folder: &'static str,
    name: String,
    version: Option<String>,
    side: Side,
    enabled: bool,
    size: u64,
    source: Option<Source>,
}

fn rows(tree: &Tree) -> Vec<FileRow> {
    let mut out: Vec<FileRow> = tree
        .files
        .iter()
        .filter_map(|(path, e)| {
            let (folder, file) = path.split_once('/')?;
            let folder = *CONTENT_FOLDERS.iter().find(|f| **f == folder)?;
            if file.contains('/') || (folder == "mods" && !is_mod_path(path)) {
                return None;
            }
            let stem = enabled_path(file).trim_end_matches(".jar").trim_end_matches(".zip");
            Some(FileRow {
                path: path.clone(),
                folder,
                name: e.name.clone().unwrap_or_else(|| stem.to_string()),
                version: e.version.clone(),
                side: e.side,
                enabled: !is_disabled(path),
                size: e.size,
                source: e.source.clone(),
            })
        })
        .collect();
    out.sort_by_key(|r| r.name.to_lowercase());
    out
}

fn scan(repo: &Repo) -> impl Future<Output = Res<Tree>> + use<> {
    let repo = repo.clone();
    blocking(move || repo.scan())
}

/// Mods, resource packs and shaders on disk. Unknown jars are looked up on Modrinth
/// (or read from their metadata) once, so they get proper names.
#[tauri::command]
pub async fn pack_files(path: String) -> Res<Vec<FileRow>> {
    let repo = open(&path)?;
    let tree = scan(&repo).await?;
    if tree.mods().any(|(_, e)| e.name.is_none() && e.source.is_none()) && repo.identify(&tree).await? > 0 {
        return Ok(rows(&scan(&repo).await?));
    }
    Ok(rows(&tree))
}

#[tauri::command]
pub async fn set_enabled(path: String, file: String, enabled: bool) -> Res<String> {
    Ok(open(&path)?.set_enabled(&file, enabled)?)
}

#[tauri::command]
pub async fn set_side(path: String, file: String, side: Side) -> Res<()> {
    let repo = open(&path)?;
    blocking(move || repo.set_side(&file, side)).await
}

#[tauri::command]
pub async fn remove_files(path: String, files: Vec<String>) -> Res<()> {
    let repo = open(&path)?;
    for f in files {
        repo.remove_file(&f)?;
    }
    Ok(())
}

/// Modrinth project ids already in the pack.
fn installed_projects(tree: &Tree) -> HashSet<String> {
    tree.files
        .values()
        .filter_map(|e| match &e.source {
            Some(Source::Modrinth { project, .. }) => Some(project.clone()),
            _ => None,
        })
        .collect()
}

/// Downloads files into the game dir and stages their metadata.
async fn download_into(app: &AppHandle, repo: &Repo, files: Vec<(String, Entry)>) -> Res<()> {
    let game = repo.game_dir();
    let items = files.iter().map(|(rel, e)| (e.clone(), game.join(rel))).collect();
    Fetcher::default().fetch_many(items, &progress(app)).await?;
    repo.stage(files.into_iter().map(|(_, e)| e))?;
    Ok(())
}

/// Installs a Modrinth project (plus required dependencies) matching the pack's
/// Minecraft version and loader. Returns the names of what was added.
#[tauri::command]
pub async fn add_project(app: AppHandle, path: String, project: String, project_type: ProjectType) -> Res<Vec<String>> {
    let repo = open(&path)?;
    let tree = scan(&repo).await?;
    let installed = installed_projects(&tree);
    if installed.contains(&project) {
        return Err(anyhow!("already in this pack").into());
    }
    // Resource packs and shaders are not loader specific.
    let loader = if project_type == ProjectType::Mod { tree.loader.kind } else { LoaderKind::Vanilla };
    let files = modrinth::resolve_install(&project, false, &tree.minecraft, loader, &installed).await?;
    let names = files.iter().map(|(rel, e)| e.name.clone().unwrap_or_else(|| rel.clone())).collect();
    download_into(&app, &repo, files).await?;
    Ok(names)
}

#[tauri::command]
pub async fn check_updates(path: String) -> Res<Vec<Update>> {
    let repo = open(&path)?;
    let tree = scan(&repo).await?;
    let (mods, other): (Vec<_>, Vec<_>) = tree.files.clone().into_iter().partition(|(p, _)| is_mod_path(p));
    let mut out = modrinth::check_updates(&mods, &tree.minecraft, tree.loader.kind).await?;
    out.extend(modrinth::check_updates(&other, &tree.minecraft, LoaderKind::Vanilla).await?);
    Ok(out)
}

/// The parts of [`Update`] needed to apply it (sent back from the UI).
#[derive(Deserialize)]
pub struct UpdateInput {
    path: String,
    filename: String,
    entry: Entry,
}

/// Downloads the new files and removes the old ones. Disabled mods stay disabled.
#[tauri::command]
pub async fn apply_updates(app: AppHandle, path: String, updates: Vec<UpdateInput>) -> Res<()> {
    let repo = open(&path)?;
    let mut files = vec![];
    let mut old = vec![];
    for u in updates {
        let folder = u.path.rsplit_once('/').map(|(f, _)| f).unwrap_or("mods");
        let suffix = if is_disabled(&u.path) { ".disabled" } else { "" };
        let rel = format!("{folder}/{}{suffix}", u.filename);
        if rel != u.path {
            old.push(u.path);
        }
        files.push((rel, u.entry));
    }
    download_into(&app, &repo, files).await?;
    for p in old {
        repo.remove_file(&p).ok();
    }
    Ok(())
}

// ---- changes ----------------------------------------------------------------------------

#[tauri::command]
pub async fn status(path: String) -> Res<Status> {
    let repo = open(&path)?;
    blocking(move || repo.status()).await
}

/// Undo unsaved changes: all of them, or just the given change ids.
#[tauri::command]
pub async fn discard(app: AppHandle, path: String, ids: Option<Vec<String>>) -> Res<()> {
    let only: Option<HashSet<String>> = ids.map(|v| v.into_iter().collect());
    open(&path)?.discard(only.as_ref(), &progress(&app)).await?;
    Ok(())
}

/// Stops tracking files: adds them to `.beaconignore`, leaving them on disk.
#[tauri::command]
pub async fn ignore_files(path: String, files: Vec<String>) -> Res<()> {
    Ok(open(&path)?.ignore(&files)?)
}

#[tauri::command]
pub async fn save_version(path: String, message: String) -> Res<String> {
    let message = if message.trim().is_empty() { "Saved changes".to_string() } else { message };
    Ok(open(&path)?.commit(&message, &author(&Settings::load())).await?)
}

#[tauri::command]
pub async fn abort_merge(app: AppHandle, path: String) -> Res<()> {
    let repo = open(&path)?;
    repo.abort_merge()?;
    repo.discard(None, &progress(&app)).await?;
    Ok(())
}

// ---- history ----------------------------------------------------------------------------

#[derive(Serialize)]
pub struct History {
    head: Option<String>,
    log: Vec<LogEntry>,
    branches: Vec<BranchInfo>,
}

#[tauri::command]
pub async fn history(path: String) -> Res<History> {
    let repo = open(&path)?;
    blocking(move || {
        let head = repo.head_commit()?;
        let log = match &head {
            Some(h) => repo.log(h, 500)?,
            None => vec![],
        };
        Ok(History { head, log, branches: repo.branches()? })
    })
    .await
}

/// Puts an old version's files back and saves that as a new version. With `publish`, also
/// releases it and deploys it to every server, so players and servers roll back too.
#[tauri::command]
pub async fn rollback(app: AppHandle, path: String, id: String, publish: bool) -> Res<ops::Rollback> {
    let repo = open(&path)?;
    let settings = Settings::load();
    let hub = if publish { Some(beacon_core::hub::Hub::authed(&settings)?) } else { None };
    Ok(ops::rollback(&repo, hub.as_ref(), &id, &author(&settings), &progress(&app)).await?)
}

/// Moves the branch back to an old version, dropping newer ones.
#[tauri::command]
pub async fn reset_version(app: AppHandle, path: String, id: String) -> Res<()> {
    Ok(open(&path)?.reset_hard(&id, &progress(&app)).await?)
}

#[tauri::command]
pub async fn switch_branch(app: AppHandle, path: String, name: String) -> Res<()> {
    Ok(open(&path)?.switch(&name, false, &progress(&app)).await?)
}

/// Creates a branch at the current version and switches to it.
#[tauri::command]
pub async fn create_branch(path: String, name: String) -> Res<()> {
    let repo = open(&path)?;
    repo.create_branch(name.trim(), None)?;
    // Same commit, so only HEAD moves; unsaved changes are kept.
    std::fs::write(repo.root.join(".beacon/HEAD"), format!("ref: refs/heads/{}\n", name.trim()))?;
    Ok(())
}

#[tauri::command]
pub async fn delete_branch(path: String, name: String) -> Res<()> {
    Ok(open(&path)?.delete_branch(&name)?)
}

/// Merges a branch into the current one. Returns conflicts for the user to pick.
/// Result of merging a branch; `fast_forward` means nothing needs saving.
#[derive(serde::Serialize)]
pub struct Merged {
    conflicts: Vec<Conflict>,
    fast_forward: bool,
}

#[tauri::command]
pub async fn merge_branch(app: AppHandle, path: String, name: String) -> Res<Merged> {
    let r = open(&path)?.merge(&name, &progress(&app)).await?;
    Ok(Merged { conflicts: r.conflicts, fast_forward: r.fast_forward })
}

#[tauri::command]
pub async fn resolve_conflicts(app: AppHandle, path: String, conflicts: Vec<Conflict>, picks: Vec<(String, Pick)>) -> Res<()> {
    Ok(open(&path)?.resolve_conflicts(&conflicts, &picks, &progress(&app)).await?)
}

// ---- upstream and subscriptions ---------------------------------------------------------

#[tauri::command]
pub async fn check_upstream(path: String) -> Res<UpstreamInfo> {
    Ok(ops::check_upstream(&open(&path)?).await?)
}

#[tauri::command]
pub async fn pull_upstream(app: AppHandle, path: String) -> Res<Vec<Conflict>> {
    Ok(ops::pull_upstream(&open(&path)?, &progress(&app)).await?.conflicts)
}

#[tauri::command]
pub async fn check_release(path: String) -> Res<UpdateInfo> {
    Ok(ops::check_release(&open(&path)?).await?)
}

#[tauri::command]
pub async fn update_subscribed(app: AppHandle, path: String) -> Res<UpdateInfo> {
    Ok(ops::update_subscribed(&open(&path)?, &progress(&app)).await?)
}

// ---- header actions ---------------------------------------------------------------------

#[tauri::command]
pub async fn launch(app: AppHandle, path: String) -> Res<()> {
    let server = match open(&path) {
        Ok(repo) => beacon_core::ops::server_address(&repo).await,
        Err(_) => None,
    };
    let session = prism::launch(Path::new(&path), &Settings::load(), server.as_deref(), &progress(&app)).await?;
    // The built-in Prism keeps its console closed when things go wrong, so the app tells the player.
    std::thread::spawn(move || {
        if let Some(failure) = session.wait() {
            app.emit("game-failed", failure).ok();
        }
    });
    Ok(())
}

/// Memory (MB) set for the instance, or null for Prism's default.
#[tauri::command]
pub async fn instance_memory(path: String) -> Res<Option<u32>> {
    Ok(instance::memory(Path::new(&path)))
}

#[tauri::command]
pub async fn set_instance_memory(path: String, mb: Option<u32>) -> Res<()> {
    if prism::busy() {
        return Err(anyhow!("Close Minecraft before changing its memory").into());
    }
    Ok(instance::set_memory(Path::new(&path), mb)?)
}

#[tauri::command]
pub async fn rename_pack(path: String, name: String) -> Res<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow!("name cannot be empty").into());
    }
    instance::set_name(Path::new(&path), name)?;
    if let Ok(repo) = open(&path) {
        let mut cfg = repo.config()?;
        cfg.name = name.to_string();
        repo.save_config(&cfg)?;
    }
    Ok(())
}

/// Deletes the whole instance folder.
#[tauri::command]
pub async fn delete_pack(path: String) -> Res<()> {
    let dir = PathBuf::from(&path);
    if !dir.join("instance.cfg").is_file() {
        return Err(anyhow!("not an instance folder").into());
    }
    blocking(move || Ok(std::fs::remove_dir_all(dir)?)).await
}

/// Opens a folder in the file manager, or a link in the browser.
#[tauri::command]
pub async fn open_external(target: String) -> Res<()> {
    if target.starts_with("http://") || target.starts_with("https://") {
        tauri_plugin_opener::open_url(&target, None::<&str>)?;
    } else {
        tauri_plugin_opener::open_path(&target, None::<&str>)?;
    }
    Ok(())
}

/// Exports the current files as a Modrinth `.mrpack`.
#[tauri::command]
pub async fn export_mrpack(path: String, out: PathBuf) -> Res<()> {
    let repo = open(&path)?;
    blocking(move || {
        let version = repo.head_commit()?.map(|c| c[..8].to_string()).context("save a version first")?;
        import::export_mrpack(&repo, &out, &version)
    })
    .await
}
