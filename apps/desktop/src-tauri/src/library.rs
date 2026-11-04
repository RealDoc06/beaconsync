//! Library level commands: listing instances, creating, importing and joining packs,
//! version lists and modpack search.

use crate::util::{Res, author, blocking, open, progress};
use anyhow::Context;
use beacon_core::{
    api::PublicPack,
    hub::{self, Hub},
    import, instance,
    instance::InstanceInfo,
    meta::{self, LoaderVersion, McVersion},
    model::{Loader, LoaderKind},
    ops,
    repo::{self, Repo, Role},
    settings::Settings,
    sources::{
        curseforge::{self, CurseForge},
        modrinth::{self, ProjectType, SearchQuery, SearchResult},
    },
};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

#[derive(Serialize)]
pub struct LibraryItem {
    #[serde(flatten)]
    info: InstanceInfo,
    mod_count: usize,
    role: Option<Role>,
    /// Name of the pack this one was forked or imported from.
    upstream: Option<String>,
}

fn mod_count(dir: &Path) -> usize {
    std::fs::read_dir(instance::game_dir(dir).join("mods"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| beacon_core::model::is_mod_path(&format!("mods/{}", e.file_name().to_string_lossy())))
        .count()
}

#[tauri::command]
pub async fn list_instances() -> Res<Vec<LibraryItem>> {
    blocking(|| {
        let roots = Settings::load().instance_roots();
        Ok(instance::list(&roots)
            .into_iter()
            .map(|info| {
                let cfg = Repo::open(&info.path).and_then(|r| r.config()).ok();
                LibraryItem {
                    mod_count: mod_count(&info.path),
                    role: cfg.as_ref().map(|c| c.role),
                    upstream: cfg.and_then(|c| c.upstream).map(|u| match u {
                        repo::Upstream::Beacon { name, .. } | repo::Upstream::Modrinth { name, .. } => name,
                    }),
                    info,
                }
            })
            .collect())
    })
    .await
}

#[derive(Serialize)]
pub struct Badges {
    /// Number of unsaved changes.
    unsaved: usize,
    /// Name of a newer release (subscribed packs) or upstream version (forks/imports).
    update: Option<String>,
}

/// Per instance state badges, computed lazily per library row.
#[tauri::command]
pub async fn instance_badges(path: String) -> Res<Badges> {
    let repo = open(&path)?;
    let r = repo.clone();
    let unsaved = blocking(move || {
        let d = r.status()?.diff;
        Ok(d.files.len() + usize::from(d.minecraft.is_some()) + usize::from(d.loader.is_some()))
    })
    .await?;
    let cfg = repo.config()?;
    let update = if cfg.role == Role::Subscriber {
        ops::check_release(&repo).await.ok().filter(|u| u.available).map(|u| u.name)
    } else if cfg.upstream.is_some() {
        ops::check_upstream(&repo).await.ok().filter(|u| u.available).map(|u| u.version)
    } else {
        None
    };
    Ok(Badges { unsaved, update })
}

/// Creates an empty instance, starts tracking it and saves the first version.
#[tauri::command]
pub async fn create_pack(name: String, minecraft: String, loader: Loader) -> Res<PathBuf> {
    let s = Settings::load();
    let dir = instance::unique_dir(&s.instances_root(), &name);
    instance::create(&dir, &name, &minecraft, &loader)?;
    let repo = Repo::init(&dir, repo::local_config(&name))?;
    repo.commit("First version", &author(&s)).await?;
    Ok(dir)
}

/// Imports a `.mrpack`, CurseForge zip or Prism export.
#[tauri::command]
pub async fn import_file(app: AppHandle, file: PathBuf) -> Res<PathBuf> {
    let s = Settings::load();
    let on = progress(&app);
    let repo = import::import_file(&file, &s.instances_root(), s.curseforge_key.as_deref(), &author(&s), &on).await?;
    Ok(repo.root)
}

/// Starts tracking an existing instance folder. Folders outside the library are copied in
/// first so they show up in the library and can be launched from Prism.
#[tauri::command]
pub async fn adopt_instance(dir: PathBuf) -> Res<PathBuf> {
    let s = Settings::load();
    if !dir.join("instance.cfg").is_file() {
        return Err(anyhow::anyhow!("This folder is not a Prism instance (no instance.cfg)").into());
    }
    let roots = s.instance_roots();
    let dir = if dir.parent().is_some_and(|p| roots.iter().any(|r| r == p)) {
        dir
    } else {
        let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "pack".into());
        let dest = instance::unique_dir(&s.instances_root(), &name);
        let (from, to) = (dir.clone(), dest.clone());
        blocking(move || copy_dir(&from, &to)).await?;
        dest
    };
    Ok(import::adopt(&dir, &author(&s)).await?.root)
}

fn copy_dir(from: &Path, to: &Path) -> anyhow::Result<()> {
    for e in walkdir::WalkDir::new(from) {
        let e = e?;
        let dest = to.join(e.path().strip_prefix(from)?);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else {
            std::fs::copy(e.path(), &dest)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn install_modrinth_pack(app: AppHandle, project: String) -> Res<PathBuf> {
    let s = Settings::load();
    let on = progress(&app);
    Ok(import::install_modrinth_pack(&project, None, &s.instances_root(), &author(&s), &on).await?.root)
}

/// Downloads the newest file of a CurseForge modpack and imports it.
#[tauri::command]
pub async fn install_curseforge_pack(app: AppHandle, id: u64) -> Res<PathBuf> {
    let s = Settings::load();
    let cf = CurseForge::new(s.curseforge_key.as_deref())?;
    let file = cf.files(id, None, None).await?.into_iter().max_by_key(|f| f.id).context("this modpack has no files")?;
    let tmp = instance::data_dir().join("downloads").join(&file.file_name);
    beacon_core::fetch::download_unverified(&file.url(), &tmp).await?;
    let on = progress(&app);
    let repo = import::import_curseforge(&tmp, &s.instances_root(), &cf, &author(&s), &on).await;
    std::fs::remove_file(&tmp).ok();
    Ok(repo?.root)
}

#[tauri::command]
pub async fn search_modrinth(
    query: String,
    project_type: ProjectType,
    minecraft: Option<String>,
    loader: Option<LoaderKind>,
    offset: u32,
) -> Res<SearchResult> {
    Ok(modrinth::search(SearchQuery { query: &query, project_type, minecraft: minecraft.as_deref(), loader, offset, limit: 30 })
        .await?)
}

#[tauri::command]
pub async fn search_curseforge_packs(query: String) -> Res<Vec<curseforge::Mod>> {
    let cf = CurseForge::new(Settings::load().curseforge_key.as_deref())?;
    Ok(cf.search(&query, curseforge::CLASS_MODPACKS, None, None).await?)
}

#[tauri::command]
pub async fn minecraft_versions() -> Res<Vec<McVersion>> {
    Ok(meta::minecraft_versions().await?)
}

#[tauri::command]
pub async fn loader_versions(kind: LoaderKind, minecraft: String) -> Res<Vec<LoaderVersion>> {
    Ok(meta::loader_versions(kind, &minecraft).await?)
}

/// Resolves a share code or link to `(hub, code)`; bare codes use the configured hub.
fn resolve_code(input: &str) -> (String, String) {
    hub::parse_link(input).unwrap_or_else(|| (Settings::load().hub_url(), hub::parse_code(input)))
}

#[derive(Serialize)]
pub struct JoinPreview {
    hub: String,
    pack: PublicPack,
}

#[tauri::command]
pub async fn preview_code(input: String) -> Res<JoinPreview> {
    let (hub, code) = resolve_code(&input);
    let pack = Hub::new(&hub, None).public_pack(&code).await?;
    Ok(JoinPreview { hub, pack })
}

/// `fork = false` follows the owner's releases, `fork = true` makes an editable copy.
#[tauri::command]
pub async fn join_pack(app: AppHandle, input: String, fork: bool) -> Res<PathBuf> {
    let s = Settings::load();
    let (hub_url, code) = resolve_code(&input);
    let token = (hub_url == s.hub_url()).then(|| s.token.clone()).flatten();
    let on = progress(&app);
    let repo = ops::join(&Hub::new(&hub_url, token), &code, &s.instances_root(), fork, &author(&s), &on).await?;
    Ok(repo.root)
}
