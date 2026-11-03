//! Wire and disk format. See `docs/ARCHITECTURE.md` for the JSON contract.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FORMAT: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind {
    #[default]
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    Neoforge,
}

impl LoaderKind {
    /// Loader names as Modrinth spells them. Quilt also runs Fabric mods.
    pub fn modrinth_loaders(self) -> &'static [&'static str] {
        match self {
            Self::Vanilla => &[],
            Self::Fabric => &["fabric"],
            Self::Quilt => &["quilt", "fabric"],
            Self::Forge => &["forge"],
            Self::Neoforge => &["neoforge"],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::Forge => "Forge",
            Self::Neoforge => "NeoForge",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Loader {
    pub kind: LoaderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    #[default]
    Both,
    Client,
    Server,
}

impl Side {
    fn is_both(&self) -> bool {
        *self == Side::Both
    }
    pub fn on_server(self) -> bool {
        self != Side::Client
    }
    pub fn on_client(self) -> bool {
        self != Side::Server
    }
}

/// Where a file can be downloaded from. Files without a source are stored as blobs.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Source {
    Modrinth { project: String, version: String, url: String },
    Curseforge { project: u64, file: u64, url: String },
    Url { url: String },
}

impl Source {
    pub fn url(&self) -> &str {
        match self {
            Self::Modrinth { url, .. } | Self::Curseforge { url, .. } | Self::Url { url } => url,
        }
    }

    /// Stable project key used to match the same mod across versions.
    pub fn project_key(&self) -> Option<String> {
        match self {
            Self::Modrinth { project, .. } => Some(format!("mr:{project}")),
            Self::Curseforge { project, .. } => Some(format!("cf:{project}")),
            Self::Url { .. } => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub hash: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Side::is_both")]
    pub side: Side,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

impl Entry {
    pub fn bare(hash: String, size: u64) -> Self {
        Self { hash, size, side: Side::Both, name: None, version: None, source: None }
    }

    /// Same bytes in the same place for the same side. Display metadata is ignored.
    pub fn same_content(&self, other: &Entry) -> bool {
        self.hash == other.hash && self.side == other.side
    }
}

/// Full snapshot of a pack: game version, loader and every tracked file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Tree {
    pub format: u32,
    pub minecraft: String,
    pub loader: Loader,
    pub files: BTreeMap<String, Entry>,
}

impl Tree {
    pub fn new(minecraft: impl Into<String>, loader: Loader) -> Self {
        Self { format: FORMAT, minecraft: minecraft.into(), loader, files: BTreeMap::new() }
    }

    /// Files keyed by identity (see [`identity`]).
    pub fn by_identity(&self) -> BTreeMap<String, (&str, &Entry)> {
        self.files.iter().map(|(p, e)| (identity(p, e), (p.as_str(), e))).collect()
    }

    /// Rejects paths that could escape the game folder. Trees from a hub are untrusted.
    pub fn validate(&self) -> anyhow::Result<()> {
        match self.files.keys().find(|p| !is_safe_path(p)) {
            Some(p) => anyhow::bail!("pack contains an unsafe file path: {p:?}"),
            None => Ok(()),
        }
    }

    pub fn mods(&self) -> impl Iterator<Item = (&String, &Entry)> {
        self.files.iter().filter(|(p, _)| is_mod_path(p))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub format: u32,
    pub tree: String,
    pub parents: Vec<String>,
    pub author: String,
    pub message: String,
    pub time: i64,
}

/// Identity of a file across versions: the source project when known, else its path
/// without the `.disabled` suffix. Lets an updated mod (new filename) diff as "updated".
pub fn identity(path: &str, entry: &Entry) -> String {
    match entry.source.as_ref().and_then(Source::project_key) {
        Some(key) => key,
        None => format!("path:{}", enabled_path(path)),
    }
}

/// Relative, `/` separated, no `..`, no drive letters or empty components.
pub fn is_safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', ':', '\0'])
        && path.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

pub fn enabled_path(path: &str) -> &str {
    path.strip_suffix(".disabled").unwrap_or(path)
}

pub fn is_disabled(path: &str) -> bool {
    path.ends_with(".disabled")
}

pub fn is_mod_path(path: &str) -> bool {
    let p = enabled_path(path);
    p.starts_with("mods/") && p.ends_with(".jar") && !p[5..].contains('/')
}

/// Paths that only matter on the client.
#[cfg(test)]
#[test]
fn unsafe_paths_are_rejected() {
    for bad in ["../x", "/etc/passwd", "mods/../../x", "C:/x", "a\\b", "a//b", ""] {
        assert!(!is_safe_path(bad), "{bad}");
    }
    assert!(is_safe_path("config/sodium-options.json"));
}

pub fn client_only_path(path: &str) -> bool {
    ["resourcepacks/", "shaderpacks/", "screenshots/"].iter().any(|p| path.starts_with(p))
}
