//! CurseForge API client. Requires an API key (from console.curseforge.com), set in Settings.

use crate::model::LoaderKind;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;

const API: &str = "https://api.curseforge.com/v1";
const MINECRAFT: u32 = 432;
pub const CLASS_MODS: u32 = 6;
pub const CLASS_MODPACKS: u32 = 4471;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Logo {
    pub thumbnail_url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Author {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Mod {
    pub id: u64,
    pub name: String,
    pub summary: String,
    pub logo: Option<Logo>,
    pub download_count: f64,
    #[serde(default)]
    pub authors: Vec<Author>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileHash {
    pub value: String,
    pub algo: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub id: u64,
    pub mod_id: u64,
    pub display_name: String,
    pub file_name: String,
    pub download_url: Option<String>,
    pub file_length: u64,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub hashes: Vec<FileHash>,
}

impl File {
    /// Download url, falling back to the CDN path for files with third party downloads off.
    pub fn url(&self) -> String {
        self.download_url.clone().unwrap_or_else(|| {
            format!(
                "https://edge.forgecdn.net/files/{}/{}/{}",
                self.id / 1000,
                self.id % 1000,
                url::form_urlencoded::byte_serialize(self.file_name.as_bytes()).collect::<String>()
            )
        })
    }
}

#[derive(Deserialize)]
struct Data<T> {
    data: T,
}

#[derive(Clone, Debug)]
pub struct CurseForge {
    key: String,
}

fn loader_type(l: LoaderKind) -> Option<u32> {
    match l {
        LoaderKind::Forge => Some(1),
        LoaderKind::Fabric => Some(4),
        LoaderKind::Quilt => Some(5),
        LoaderKind::Neoforge => Some(6),
        LoaderKind::Vanilla => None,
    }
}

impl CurseForge {
    pub fn new(key: Option<&str>) -> Result<Self> {
        match key.filter(|k| !k.trim().is_empty()) {
            Some(k) => Ok(Self { key: k.trim().to_string() }),
            None => bail!("Add a CurseForge API key in Settings to use CurseForge"),
        }
    }

    async fn send<T: serde::de::DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> Result<T> {
        let resp = req.header("x-api-key", &self.key).send().await?;
        if resp.status() == 403 {
            bail!("CurseForge rejected the API key");
        }
        Ok(resp.error_for_status()?.json::<Data<T>>().await?.data)
    }

    pub async fn search(&self, query: &str, class: u32, minecraft: Option<&str>, loader: Option<LoaderKind>) -> Result<Vec<Mod>> {
        let mut q = vec![
            ("gameId", MINECRAFT.to_string()),
            ("classId", class.to_string()),
            ("searchFilter", query.to_string()),
            ("sortField", "2".into()),
            ("sortOrder", "desc".into()),
            ("pageSize", "30".into()),
        ];
        if let Some(mc) = minecraft {
            q.push(("gameVersion", mc.to_string()));
        }
        if let Some(t) = loader.and_then(loader_type) {
            q.push(("modLoaderType", t.to_string()));
        }
        self.send(crate::http().get(format!("{API}/mods/search")).query(&q)).await
    }

    pub async fn files(&self, mod_id: u64, minecraft: Option<&str>, loader: Option<LoaderKind>) -> Result<Vec<File>> {
        let mut q = vec![("pageSize", "50".to_string())];
        if let Some(mc) = minecraft {
            q.push(("gameVersion", mc.to_string()));
        }
        if let Some(t) = loader.and_then(loader_type) {
            q.push(("modLoaderType", t.to_string()));
        }
        self.send(crate::http().get(format!("{API}/mods/{mod_id}/files")).query(&q)).await
    }

    pub async fn get_files(&self, ids: &[u64]) -> Result<Vec<File>> {
        self.send(crate::http().post(format!("{API}/mods/files")).json(&json!({ "fileIds": ids }))).await
    }

    pub async fn get_mods(&self, ids: &[u64]) -> Result<Vec<Mod>> {
        self.send(crate::http().post(format!("{API}/mods")).json(&json!({ "modIds": ids }))).await
    }
}
