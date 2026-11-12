//! Importing packs from other sources and exporting to `.mrpack`.

use crate::{
    fetch::OnProgress,
    hash, instance,
    model::{Entry, Loader, LoaderKind, Side, Source, Tree, client_only_path},
    repo::{self, PackConfig, Repo, Upstream},
    sources::{curseforge::CurseForge, modrinth},
    store::ObjectStore,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MrHashes {
    pub sha1: String,
    pub sha512: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MrEnv {
    pub client: String,
    pub server: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MrFile {
    pub path: String,
    pub hashes: MrHashes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<MrEnv>,
    pub downloads: Vec<String>,
    pub file_size: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MrIndex {
    pub format_version: u32,
    pub game: String,
    pub version_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub files: Vec<MrFile>,
    pub dependencies: HashMap<String, String>,
}

/// What kind of file the user picked to import.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImportKind {
    Mrpack,
    Curseforge,
    Prism,
}

pub fn detect(path: &Path) -> Result<ImportKind> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path)?).context("not a zip file")?;
    let names: Vec<String> = zip.file_names().map(str::to_string).collect();
    if names.iter().any(|n| n == "modrinth.index.json") {
        return Ok(ImportKind::Mrpack);
    }
    if names.iter().any(|n| n == "manifest.json") {
        let mut s = String::new();
        zip.by_name("manifest.json")?.read_to_string(&mut s)?;
        if s.contains("\"minecraft\"") {
            return Ok(ImportKind::Curseforge);
        }
    }
    if names.iter().any(|n| n.ends_with("instance.cfg")) {
        return Ok(ImportKind::Prism);
    }
    bail!("unrecognised pack format (expected .mrpack, CurseForge zip or Prism export)")
}

fn loader_from_deps(deps: &HashMap<String, String>) -> Loader {
    for (key, kind) in [
        ("fabric-loader", LoaderKind::Fabric),
        ("quilt-loader", LoaderKind::Quilt),
        ("neoforge", LoaderKind::Neoforge),
        ("forge", LoaderKind::Forge),
    ] {
        if let Some(v) = deps.get(key) {
            return Loader { kind, version: Some(v.clone()) };
        }
    }
    Loader::default()
}

/// `https://cdn.modrinth.com/data/<project>/versions/<version>/<file>` -> Modrinth source.
fn source_from_url(url: &str) -> Source {
    let parts: Vec<&str> = url.split('/').collect();
    if let Some(i) = parts.iter().position(|p| *p == "data")
        && url.starts_with("https://cdn.modrinth.com/")
        && parts.get(i + 2) == Some(&"versions")
        && let (Some(project), Some(version)) = (parts.get(i + 1), parts.get(i + 3))
    {
        return Source::Modrinth { project: project.to_string(), version: version.to_string(), url: url.to_string() };
    }
    Source::Url { url: url.to_string() }
}

fn safe_rel(path: &str) -> Option<String> {
    let p = path.replace('\\', "/");
    let bad = p.starts_with('/') || p.split('/').any(|c| c == ".." || c.is_empty()) || p.contains(':');
    (!bad).then_some(p)
}

/// Reads an `.mrpack` into a tree. Override files are stored as blobs in `store`.
pub async fn mrpack_tree(path: &Path, store: &ObjectStore) -> Result<(MrIndex, Tree)> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
    let index: MrIndex = {
        let mut s = String::new();
        zip.by_name("modrinth.index.json").context("missing modrinth.index.json")?.read_to_string(&mut s)?;
        serde_json::from_str(&s)?
    };
    let mc = index.dependencies.get("minecraft").cloned().context("pack has no minecraft version")?;
    let mut tree = Tree::new(mc, loader_from_deps(&index.dependencies));
    let mut project_ids = vec![];
    for f in &index.files {
        let Some(rel) = safe_rel(&f.path) else { continue };
        let Some(url) = f.downloads.first() else { continue };
        let side = match &f.env {
            Some(e) if e.server == "unsupported" => Side::Client,
            Some(e) if e.client == "unsupported" => Side::Server,
            _ if client_only_path(&rel) => Side::Client,
            _ => Side::Both,
        };
        let source = source_from_url(url);
        if let Source::Modrinth { project, .. } = &source {
            project_ids.push(project.clone());
        }
        tree.files.insert(
            rel,
            Entry { hash: f.hashes.sha512.clone(), size: f.file_size, side, name: None, version: None, source: Some(source) },
        );
    }
    // Overrides are copied in order, later folders win (client/server overrides).
    for prefix in ["overrides/", "client-overrides/", "server-overrides/"] {
        for i in 0..zip.len() {
            let mut file = zip.by_index(i)?;
            if !file.is_file() {
                continue;
            }
            let Some(rel) = file.name().strip_prefix(prefix).and_then(safe_rel) else { continue };
            let mut bytes = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut bytes)?;
            let id = store.put(&bytes)?;
            let side = match prefix {
                "client-overrides/" => Side::Client,
                "server-overrides/" => Side::Server,
                _ if client_only_path(&rel) => Side::Client,
                _ => Side::Both,
            };
            tree.files.insert(rel, Entry { side, ..Entry::bare(id, bytes.len() as u64) });
        }
    }
    // Best effort display names and version numbers.
    project_ids.sort();
    project_ids.dedup();
    let version_ids: Vec<String> = tree
        .files
        .values()
        .filter_map(|e| match &e.source {
            Some(Source::Modrinth { version, .. }) => Some(version.clone()),
            _ => None,
        })
        .collect();
    let (projects, versions) = futures::join!(modrinth::projects(&project_ids), modrinth::versions_by_id(&version_ids));
    let titles: HashMap<String, String> = projects.unwrap_or_default().into_iter().map(|p| (p.id, p.title)).collect();
    let numbers: HashMap<String, String> = versions.unwrap_or_default().into_iter().map(|v| (v.id, v.version_number)).collect();
    for e in tree.files.values_mut() {
        if let Some(Source::Modrinth { project, version, .. }) = &e.source {
            e.name = titles.get(project).cloned();
            e.version = numbers.get(version).cloned();
        }
    }
    Ok((index, tree))
}

/// Creates a new instance + repo from a tree and saves the first version.
pub async fn materialize(
    dir: &Path,
    config: PackConfig,
    tree: &Tree,
    message: &str,
    author: &str,
    on: OnProgress<'_>,
) -> Result<Repo> {
    instance::create(dir, &config.name, &tree.minecraft, &tree.loader)?;
    let repo = Repo::init(dir, config)?;
    repo.checkout_tree(tree, None, on).await?;
    repo.commit(message, author).await?;
    Ok(repo)
}

/// Imports a `.mrpack`. When `upstream` is known (installed from Modrinth) it is recorded so
/// the pack can later pull new modpack versions.
pub async fn import_mrpack(
    file: &Path,
    root: &Path,
    upstream: Option<(String, String)>,
    author: &str,
    on: OnProgress<'_>,
) -> Result<Repo> {
    let staging = ObjectStore::new(crate::instance::data_dir().join("import-cache"));
    let (index, tree) = mrpack_tree(file, &staging).await?;
    let dir = instance::unique_dir(root, &index.name);
    let upstream = match upstream {
        Some(u) => Some(u),
        None => {
            let h = hash::sha512_file(file)?;
            modrinth::identify(&[h]).await.ok().and_then(|m| m.into_values().next()).and_then(|e| match e.source {
                Some(Source::Modrinth { project, version, .. }) => Some((project, version)),
                _ => None,
            })
        }
    };
    let config = PackConfig { name: index.name.clone(), ..repo::local_config(&index.name) };
    // Blobs from the staging store must be visible to the new repo during checkout.
    instance::create(&dir, &config.name, &tree.minecraft, &tree.loader)?;
    let repo = Repo::init(&dir, config)?;
    copy_blobs(&staging, &repo.store, &tree)?;
    let base_tree = repo.store_tree(&tree)?;
    if let Some((project, version)) = upstream {
        let mut cfg = repo.config()?;
        cfg.upstream = Some(Upstream::Modrinth { project, name: index.name.clone(), version, base_tree });
        repo.save_config(&cfg)?;
    }
    repo.checkout_tree(&tree, None, on).await?;
    repo.commit(&format!("Imported {} {}", index.name, index.version_id), author).await?;
    fs::remove_dir_all(staging_path()).ok();
    Ok(repo)
}

fn staging_path() -> PathBuf {
    crate::instance::data_dir().join("import-cache")
}

fn copy_blobs(from: &ObjectStore, to: &ObjectStore, tree: &Tree) -> Result<()> {
    for e in tree.files.values().filter(|e| e.source.is_none()) {
        if from.has(&e.hash) {
            to.put_verified(&e.hash, &from.get(&e.hash)?)?;
        }
    }
    Ok(())
}

/// Downloads and imports the newest (or a given) version of a Modrinth modpack.
pub async fn install_modrinth_pack(
    project: &str,
    version: Option<&str>,
    root: &Path,
    author: &str,
    on: OnProgress<'_>,
) -> Result<Repo> {
    let v = match version {
        Some(id) => modrinth::version(id).await?,
        None => modrinth::versions(project, None, None).await?.into_iter().next().context("modpack has no versions")?,
    };
    let file = v.primary_file().context("modpack version has no file")?;
    let tmp = crate::instance::data_dir().join("downloads").join(&file.filename);
    crate::fetch::download(&file.url, &file.hashes.sha512, &tmp).await?;
    let repo = import_mrpack(&tmp, root, Some((v.project_id.clone(), v.id.clone())), author, on).await;
    fs::remove_file(&tmp).ok();
    repo
}

/// Imports a CurseForge modpack zip (needs an API key to resolve mod downloads).
pub async fn import_curseforge(file: &Path, root: &Path, cf: &CurseForge, author: &str, on: OnProgress<'_>) -> Result<Repo> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CfLoader {
        id: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CfMc {
        version: String,
        mod_loaders: Vec<CfLoader>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CfFile {
        #[serde(rename = "projectID")]
        project_id: u64,
        #[serde(rename = "fileID")]
        file_id: u64,
    }
    #[derive(Deserialize)]
    struct CfManifest {
        minecraft: CfMc,
        name: String,
        #[serde(default)]
        version: String,
        files: Vec<CfFile>,
        #[serde(default)]
        overrides: Option<String>,
    }
    let mut zip = zip::ZipArchive::new(fs::File::open(file)?)?;
    let manifest: CfManifest = {
        let mut s = String::new();
        zip.by_name("manifest.json")?.read_to_string(&mut s)?;
        serde_json::from_str(&s)?
    };
    let loader = manifest
        .minecraft
        .mod_loaders
        .first()
        .and_then(|l| l.id.split_once('-'))
        .map(|(kind, v)| Loader {
            kind: match kind {
                "fabric" => LoaderKind::Fabric,
                "quilt" => LoaderKind::Quilt,
                "neoforge" => LoaderKind::Neoforge,
                _ => LoaderKind::Forge,
            },
            version: Some(v.to_string()),
        })
        .unwrap_or_default();
    let dir = instance::unique_dir(root, &manifest.name);
    instance::create(&dir, &manifest.name, &manifest.minecraft.version, &loader)?;
    let repo = Repo::init(&dir, repo::local_config(&manifest.name))?;
    let game = repo.game_dir();

    let overrides = format!("{}/", manifest.overrides.as_deref().unwrap_or("overrides"));
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        let Some(rel) = f.name().strip_prefix(&overrides).and_then(safe_rel) else { continue };
        if f.is_file() {
            let mut bytes = vec![];
            f.read_to_end(&mut bytes)?;
            crate::store::write_atomic(&game.join(rel), &bytes)?;
        }
    }

    let ids: Vec<u64> = manifest.files.iter().map(|f| f.file_id).collect();
    let mut files = vec![];
    for chunk in ids.chunks(100) {
        files.extend(cf.get_files(chunk).await?);
    }
    let names: HashMap<u64, String> = cf
        .get_mods(&manifest.files.iter().map(|f| f.project_id).collect::<Vec<_>>())
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| (m.id, m.name))
        .collect();
    let total = files.len();
    let mut staged = vec![];
    for (i, f) in files.into_iter().enumerate() {
        let url = f.url();
        let dest = game.join("mods").join(&f.file_name);
        let sha512 =
            crate::fetch::download_unverified(&url, &dest).await.with_context(|| format!("downloading {}", f.file_name))?;
        let meta = repo::read_jar_meta(&dest).unwrap_or_default();
        staged.push(Entry {
            hash: sha512,
            size: f.file_length,
            side: meta.side.unwrap_or_default(),
            name: names.get(&f.mod_id).cloned().or(meta.name),
            version: meta.version.or(Some(f.display_name.clone())),
            source: Some(Source::Curseforge { project: f.mod_id, file: f.id, url }),
        });
        on(crate::fetch::Progress { done: i + 1, total, label: f.display_name });
    }
    repo.stage(staged)?;
    repo.commit(&format!("Imported {} {}", manifest.name, manifest.version), author).await?;
    Ok(repo)
}

/// Extracts a Prism/MultiMC instance export (.zip) and starts tracking it.
pub async fn import_prism_zip(file: &Path, root: &Path, author: &str) -> Result<Repo> {
    let mut zip = zip::ZipArchive::new(fs::File::open(file)?)?;
    let cfg_name = zip.file_names().find(|n| n.ends_with("instance.cfg")).context("no instance.cfg")?.to_string();
    let prefix = cfg_name.trim_end_matches("instance.cfg").to_string();
    let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "pack".into());
    let dir = instance::unique_dir(root, &stem);
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        let Some(rel) = f.name().strip_prefix(&prefix).and_then(safe_rel) else { continue };
        if f.is_file() {
            let mut bytes = vec![];
            f.read_to_end(&mut bytes)?;
            crate::store::write_atomic(&dir.join(rel), &bytes)?;
        }
    }
    instance::make_portable(&dir)?;
    adopt(&dir, author).await
}

/// Starts tracking an existing instance folder (keeps its repo if it already has one).
pub async fn adopt(dir: &Path, author: &str) -> Result<Repo> {
    if Repo::is_repo(dir) {
        return Repo::open(dir);
    }
    let name = instance::read_name(dir).unwrap_or_else(|| "Pack".into());
    let repo = Repo::init(dir, repo::local_config(&name))?;
    repo.commit("First version", author).await?;
    Ok(repo)
}

/// Imports any supported file, choosing the importer from its contents.
pub async fn import_file(file: &Path, root: &Path, cf_key: Option<&str>, author: &str, on: OnProgress<'_>) -> Result<Repo> {
    match detect(file)? {
        ImportKind::Mrpack => import_mrpack(file, root, None, author, on).await,
        ImportKind::Curseforge => import_curseforge(file, root, &CurseForge::new(cf_key)?, author, on).await,
        ImportKind::Prism => import_prism_zip(file, root, author).await,
    }
}

/// Folders of an installed server that make up its pack. Worlds, libraries and the server's
/// own settings (`server.properties`, ops, whitelist) stay with the server.
pub const SERVER_PACK_DIRS: &[&str] = &["mods", "config", "defaultconfigs", "kubejs", "scripts"];

/// Name of the most recently written subfolder: the version in use when a server was upgraded.
/// `None` when `dir` doesn't exist; unreadable folders are errors, not a missing loader.
fn newest_child(dir: &Path) -> Result<Option<String>> {
    let entries = match fs::read_dir(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        r => r.with_context(|| format!("reading {}", dir.display()))?,
    };
    let mut newest = None;
    for e in entries {
        let e = e?;
        let meta = e.metadata().with_context(|| format!("reading {}", e.path().display()))?;
        if !meta.is_dir() {
            continue;
        }
        let modified = meta.modified()?;
        if newest.as_ref().is_none_or(|(t, _)| modified > *t) {
            newest = Some((modified, e.file_name().to_string_lossy().into_owned()));
        }
    }
    Ok(newest.map(|(_, name)| name))
}

/// Minecraft version and loader of an installed server, read from what the server and the
/// loader installers leave behind: `libraries/<loader>/<version>` and `versions/<minecraft>`.
pub fn detect_server(dir: &Path) -> Result<(String, Loader)> {
    let lib = |path: &str| newest_child(&dir.join("libraries").join(path));
    let mut minecraft = None;
    let loader = if let Some(v) = lib("net/fabricmc/fabric-loader")? {
        Loader { kind: LoaderKind::Fabric, version: Some(v) }
    } else if let Some(v) = lib("org/quiltmc/quilt-loader")? {
        Loader { kind: LoaderKind::Quilt, version: Some(v) }
    } else if let Some(v) = lib("net/neoforged/neoforge")? {
        Loader { kind: LoaderKind::Neoforge, version: Some(v) }
    } else if let Some((mc, v)) = lib("net/minecraftforge/forge")?.as_deref().and_then(|f| f.split_once('-')) {
        minecraft = Some(mc.to_string());
        Loader { kind: LoaderKind::Forge, version: Some(v.to_string()) }
    } else {
        Loader::default()
    };
    // (Neo)Forge servers keep the game in `libraries/net/minecraft/server/<mc>[-<mcp>]`.
    let minecraft = match minecraft {
        Some(mc) => Some(mc),
        None => newest_child(&dir.join("versions"))?,
    };
    let minecraft = match minecraft {
        Some(mc) => Some(mc),
        None => lib("net/minecraft/server")?.map(|v| v.split('-').next().unwrap_or_default().to_string()),
    };
    let minecraft = minecraft.filter(|v| !v.is_empty()).context("can't tell which Minecraft version this server runs")?;
    Ok((minecraft, loader))
}

/// A pack name for a server: its MOTD when it is plain text, else the folder name.
pub fn server_name(dir: &Path) -> String {
    let props = fs::read_to_string(dir.join("server.properties")).unwrap_or_default();
    let motd = props.lines().find_map(|l| l.strip_prefix("motd=")).map(str::trim).unwrap_or_default();
    let plain = !motd.is_empty() && motd != "A Minecraft Server" && !motd.contains(['\\', '§']);
    if plain {
        return motd.to_string();
    }
    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Server".into())
}

/// Creates a pack from an installed server: a new instance in `root` with copies of the
/// server's [`SERVER_PACK_DIRS`], saved as the first version. The server is only read.
pub async fn import_server(
    server: &Path,
    root: &Path,
    name: &str,
    minecraft: &str,
    loader: &Loader,
    author: &str,
) -> Result<Repo> {
    let dir = instance::unique_dir(root, name);
    instance::create(&dir, name, minecraft, loader)?;
    let game = instance::game_dir(&dir);
    for folder in SERVER_PACK_DIRS {
        let from = server.join(folder);
        if !from.try_exists().with_context(|| format!("reading {}", from.display()))? {
            continue;
        }
        // Linked mods and configs are part of what the server runs, so links are followed.
        for e in walkdir::WalkDir::new(&from).follow_links(true) {
            let e = e.with_context(|| format!("reading {}", from.display()))?;
            if !e.file_type().is_file() {
                continue;
            }
            let dest = game.join(folder).join(e.path().strip_prefix(&from)?);
            fs::create_dir_all(dest.parent().context("no parent")?)?;
            fs::copy(e.path(), &dest).with_context(|| format!("copying {}", e.path().display()))?;
        }
    }
    let repo = Repo::init(&dir, repo::local_config(name))?;
    repo.commit(&format!("Imported from the server at {}", server.display()), author).await?;
    Ok(repo)
}

const MRPACK_HOSTS: &[&str] = &["cdn.modrinth.com", "github.com", "raw.githubusercontent.com", "gitlab.com"];

/// Exports the current working tree as a Modrinth `.mrpack`.
pub fn export_mrpack(repo: &Repo, out: &Path, version_id: &str) -> Result<()> {
    let tree = repo.scan()?;
    let game = repo.game_dir();
    let name = repo.config()?.name;
    let mut deps = HashMap::from([("minecraft".to_string(), tree.minecraft.clone())]);
    if let Some(v) = &tree.loader.version {
        let key = match tree.loader.kind {
            LoaderKind::Fabric => Some("fabric-loader"),
            LoaderKind::Quilt => Some("quilt-loader"),
            LoaderKind::Forge => Some("forge"),
            LoaderKind::Neoforge => Some("neoforge"),
            LoaderKind::Vanilla => None,
        };
        if let Some(k) = key {
            deps.insert(k.to_string(), v.clone());
        }
    }
    let mut zip = zip::ZipWriter::new(fs::File::create(out)?);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut files = vec![];
    for (path, e) in &tree.files {
        if crate::model::is_disabled(path) {
            continue;
        }
        let downloadable =
            e.source.as_ref().map(|s| s.url()).filter(|u| {
                url::Url::parse(u).ok().and_then(|u| u.host_str().map(|h| MRPACK_HOSTS.contains(&h))).unwrap_or(false)
            });
        let bytes = fs::read(game.join(path))?;
        match downloadable {
            Some(url) => files.push(MrFile {
                path: path.clone(),
                hashes: MrHashes { sha1: hex::encode(Sha1::digest(&bytes)), sha512: e.hash.clone() },
                env: Some(MrEnv {
                    client: if e.side.on_client() { "required" } else { "unsupported" }.into(),
                    server: if e.side.on_server() { "required" } else { "unsupported" }.into(),
                }),
                downloads: vec![url.to_string()],
                file_size: e.size,
            }),
            None => {
                let folder = match e.side {
                    Side::Client => "client-overrides",
                    Side::Server => "server-overrides",
                    Side::Both => "overrides",
                };
                zip.start_file(format!("{folder}/{path}"), opts)?;
                zip.write_all(&bytes)?;
            }
        }
    }
    let index = MrIndex {
        format_version: 1,
        game: "minecraft".into(),
        version_id: version_id.into(),
        name,
        summary: None,
        files,
        dependencies: deps,
    };
    zip.start_file("modrinth.index.json", opts)?;
    zip.write_all(&serde_json::to_vec_pretty(&index)?)?;
    zip.finish()?;
    Ok(())
}

#[cfg(test)]
mod server_tests {
    use super::*;

    fn mkdirs(root: &Path, dirs: &[&str]) {
        for d in dirs {
            fs::create_dir_all(root.join(d)).unwrap();
        }
    }

    #[test]
    fn detects_installed_servers() {
        let fabric = tempfile::tempdir().unwrap();
        mkdirs(fabric.path(), &["libraries/net/fabricmc/fabric-loader/0.20.1", "versions/26.3"]);
        let forge = tempfile::tempdir().unwrap();
        mkdirs(forge.path(), &["libraries/net/minecraftforge/forge/1.20.1-47.3.0"]);
        let neoforge = tempfile::tempdir().unwrap();
        mkdirs(
            neoforge.path(),
            &["libraries/net/neoforged/neoforge/21.1.250", "libraries/net/minecraft/server/1.21.1-20240808.144430"],
        );
        let empty = tempfile::tempdir().unwrap();

        let loader = |kind, v: &str| Loader { kind, version: Some(v.into()) };
        assert_eq!(detect_server(fabric.path()).unwrap(), ("26.3".into(), loader(LoaderKind::Fabric, "0.20.1")));
        assert_eq!(detect_server(forge.path()).unwrap(), ("1.20.1".into(), loader(LoaderKind::Forge, "47.3.0")));
        assert_eq!(detect_server(neoforge.path()).unwrap(), ("1.21.1".into(), loader(LoaderKind::Neoforge, "21.1.250")));
        assert!(detect_server(empty.path()).is_err());
    }
}
