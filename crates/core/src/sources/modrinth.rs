//! Modrinth API v2 client (no key needed).

use crate::model::{Entry, LoaderKind, Side, Source};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};

const API: &str = "https://api.modrinth.com/v2";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProjectType {
    Mod,
    Modpack,
    Resourcepack,
    Shader,
    Datapack,
    Plugin,
}

impl ProjectType {
    /// Folder (inside the game dir) where files of this type live.
    pub fn folder(self) -> &'static str {
        match self {
            Self::Resourcepack => "resourcepacks",
            Self::Shader => "shaderpacks",
            Self::Datapack => "datapacks",
            _ => "mods",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub author: String,
    pub project_type: ProjectType,
    #[serde(default)]
    pub client_side: String,
    #[serde(default)]
    pub server_side: String,
    #[serde(default)]
    pub categories: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    pub total_hits: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub project_type: ProjectType,
    #[serde(default)]
    pub client_side: String,
    #[serde(default)]
    pub server_side: String,
}

impl Project {
    pub fn side(&self) -> Side {
        side_of(&self.client_side, &self.server_side)
    }
}

fn side_of(client: &str, server: &str) -> Side {
    match (client, server) {
        (_, "unsupported") => Side::Client,
        ("unsupported", _) => Side::Server,
        _ => Side::Both,
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Hashes {
    pub sha1: String,
    pub sha512: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
    pub hashes: Hashes,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Dependency {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub dependency_type: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Version {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<VersionFile>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    pub date_published: String,
    #[serde(default)]
    pub changelog: Option<String>,
}

impl Version {
    pub fn primary_file(&self) -> Option<&VersionFile> {
        self.files.iter().find(|f| f.primary).or(self.files.first())
    }

    /// Tree entry for this version's primary file.
    pub fn entry(&self, title: Option<&str>, side: Side) -> Option<(String, Entry)> {
        let f = self.primary_file()?;
        Some((
            f.filename.clone(),
            Entry {
                hash: f.hashes.sha512.clone(),
                size: f.size,
                side,
                name: title.map(str::to_string),
                version: Some(self.version_number.clone()),
                source: Some(Source::Modrinth {
                    project: self.project_id.clone(),
                    version: self.id.clone(),
                    url: f.url.clone(),
                }),
            },
        ))
    }
}

async fn get<T: serde::de::DeserializeOwned>(path: &str, query: &[(&str, String)]) -> Result<T> {
    Ok(crate::http().get(format!("{API}{path}")).query(query).send().await?.error_for_status()?.json().await?)
}

async fn post<T: serde::de::DeserializeOwned>(path: &str, body: serde_json::Value) -> Result<T> {
    Ok(crate::http().post(format!("{API}{path}")).json(&body).send().await?.error_for_status()?.json().await?)
}

pub struct SearchQuery<'a> {
    pub query: &'a str,
    pub project_type: ProjectType,
    pub minecraft: Option<&'a str>,
    pub loader: Option<LoaderKind>,
    pub offset: u32,
    pub limit: u32,
}

pub async fn search(q: SearchQuery<'_>) -> Result<SearchResult> {
    let ty = match q.project_type {
        ProjectType::Mod => "mod",
        ProjectType::Modpack => "modpack",
        ProjectType::Resourcepack => "resourcepack",
        ProjectType::Shader => "shader",
        ProjectType::Datapack => "datapack",
        ProjectType::Plugin => "plugin",
    };
    let mut facets = vec![vec![format!("project_type:{ty}")]];
    if let Some(mc) = q.minecraft {
        facets.push(vec![format!("versions:{mc}")]);
    }
    if let Some(l) = q.loader.filter(|_| matches!(q.project_type, ProjectType::Mod | ProjectType::Modpack)) {
        let cats: Vec<String> = l.modrinth_loaders().iter().map(|l| format!("categories:{l}")).collect();
        if !cats.is_empty() {
            facets.push(cats);
        }
    }
    get(
        "/search",
        &[
            ("query", q.query.to_string()),
            ("facets", serde_json::to_string(&facets)?),
            ("offset", q.offset.to_string()),
            ("limit", q.limit.to_string()),
            ("index", if q.query.is_empty() { "downloads" } else { "relevance" }.to_string()),
        ],
    )
    .await
}

pub async fn project(id: &str) -> Result<Project> {
    get(&format!("/project/{id}"), &[]).await
}

pub async fn projects(ids: &[String]) -> Result<Vec<Project>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    get("/projects", &[("ids", serde_json::to_string(ids)?)]).await
}

/// Versions of a project, newest first, optionally filtered by game version and loader.
pub async fn versions(project: &str, minecraft: Option<&str>, loader: Option<LoaderKind>) -> Result<Vec<Version>> {
    let mut q = vec![];
    if let Some(mc) = minecraft {
        q.push(("game_versions", serde_json::to_string(&[mc])?));
    }
    if let Some(l) = loader.filter(|l| *l != LoaderKind::Vanilla) {
        q.push(("loaders", serde_json::to_string(l.modrinth_loaders())?));
    }
    get(&format!("/project/{project}/version"), &q).await
}

/// Several versions at once (Modrinth caps this at a few hundred ids per call).
pub async fn versions_by_id(ids: &[String]) -> Result<Vec<Version>> {
    let mut out = vec![];
    for chunk in ids.chunks(200) {
        out.extend(get::<Vec<Version>>("/versions", &[("ids", serde_json::to_string(chunk)?)]).await?);
    }
    Ok(out)
}

pub async fn version(id: &str) -> Result<Version> {
    get(&format!("/version/{id}"), &[]).await
}

/// Maps SHA-512 hashes of local files to tree entries with a Modrinth source.
pub async fn identify(hashes: &[String]) -> Result<HashMap<String, Entry>> {
    if hashes.is_empty() {
        return Ok(HashMap::new());
    }
    let found: HashMap<String, Version> =
        post("/version_files", json!({ "hashes": hashes, "algorithm": "sha512" })).await?;
    let ids: Vec<String> = found.values().map(|v| v.project_id.clone()).collect::<HashSet<_>>().into_iter().collect();
    let projects: HashMap<String, Project> = projects(&ids).await?.into_iter().map(|p| (p.id.clone(), p)).collect();
    Ok(found
        .into_iter()
        .filter_map(|(hash, v)| {
            let p = projects.get(&v.project_id);
            let file = v.files.iter().find(|f| f.hashes.sha512 == hash)?;
            let entry = Entry {
                hash: hash.clone(),
                size: file.size,
                side: p.map(Project::side).unwrap_or_default(),
                name: p.map(|p| p.title.clone()),
                version: Some(v.version_number.clone()),
                source: Some(Source::Modrinth { project: v.project_id.clone(), version: v.id.clone(), url: file.url.clone() }),
            };
            Some((hash, entry))
        })
        .collect())
}

#[derive(Serialize, Clone, Debug)]
pub struct Update {
    /// Current path of the file being replaced.
    pub path: String,
    pub current: Option<String>,
    /// New file name (inside the same folder) and entry.
    pub filename: String,
    pub entry: Entry,
}

/// Finds newer compatible versions for Modrinth sourced files.
pub async fn check_updates(files: &[(String, Entry)], minecraft: &str, loader: LoaderKind) -> Result<Vec<Update>> {
    let tracked: Vec<&(String, Entry)> =
        files.iter().filter(|(_, e)| matches!(e.source, Some(Source::Modrinth { .. }))).collect();
    if tracked.is_empty() {
        return Ok(vec![]);
    }
    let hashes: Vec<&str> = tracked.iter().map(|(_, e)| e.hash.as_str()).collect();
    let mut body = json!({ "hashes": hashes, "algorithm": "sha512", "game_versions": [minecraft] });
    if loader != LoaderKind::Vanilla {
        body["loaders"] = json!(loader.modrinth_loaders());
    }
    let latest: HashMap<String, Version> = post("/version_files/update", body).await?;
    let mut out = vec![];
    for (path, e) in tracked {
        let Some(v) = latest.get(&e.hash) else { continue };
        let Some(Source::Modrinth { version, .. }) = &e.source else { continue };
        if &v.id == version {
            continue;
        }
        if let Some((filename, entry)) = v.entry(e.name.as_deref(), e.side) {
            out.push(Update { path: path.clone(), current: e.version.clone(), filename, entry });
        }
    }
    Ok(out)
}

/// Picks a version (explicit id, or newest compatible) and resolves required dependencies
/// that are not installed yet. Returns `(folder/filename, entry)` pairs to download.
pub async fn resolve_install(
    project_or_version: &str,
    explicit_version: bool,
    minecraft: &str,
    loader: LoaderKind,
    installed_projects: &HashSet<String>,
) -> Result<Vec<(String, Entry)>> {
    let mut out = vec![];
    let mut seen: HashSet<String> = installed_projects.clone();
    let mut queue: Vec<(Option<String>, Option<String>)> = vec![if explicit_version {
        (None, Some(project_or_version.to_string()))
    } else {
        (Some(project_or_version.to_string()), None)
    }];
    let mut picked = vec![];
    while let Some((project, version_id)) = queue.pop() {
        let v = match (project, version_id) {
            (_, Some(vid)) => version(&vid).await?,
            (Some(pid), None) => versions(&pid, Some(minecraft), Some(loader))
                .await?
                .into_iter()
                .next()
                .with_context(|| format!("no version of {pid} for Minecraft {minecraft} ({})", loader.label()))?,
            (None, None) => continue,
        };
        if !seen.insert(v.project_id.clone()) && !picked.is_empty() {
            continue;
        }
        for d in v.dependencies.iter().filter(|d| d.dependency_type == "required") {
            if d.project_id.as_ref().is_some_and(|p| seen.contains(p)) {
                continue;
            }
            queue.push((d.project_id.clone(), d.version_id.clone()));
        }
        picked.push(v);
    }
    let ids: Vec<String> = picked.iter().map(|v| v.project_id.clone()).collect();
    let projects: HashMap<String, Project> = projects(&ids).await?.into_iter().map(|p| (p.id.clone(), p)).collect();
    for v in picked {
        let p = projects.get(&v.project_id);
        let folder = p.map(|p| p.project_type.folder()).unwrap_or("mods");
        let side = p.map(Project::side).unwrap_or_default();
        if let Some((file, entry)) = v.entry(p.map(|p| p.title.as_str()), side) {
            out.push((format!("{folder}/{file}"), entry));
        }
    }
    Ok(out)
}
