//! Minecraft and loader version lists.

use crate::model::LoaderKind;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct McVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
    pub url: String,
}

#[derive(Deserialize)]
struct Manifest {
    versions: Vec<McVersion>,
}

pub async fn minecraft_versions() -> Result<Vec<McVersion>> {
    let m: Manifest = crate::http()
        .get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(m.versions)
}

#[derive(Serialize, Clone, Debug)]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
}

async fn json(url: &str) -> Result<serde_json::Value> {
    Ok(crate::http().get(url).send().await?.error_for_status()?.json().await?)
}

/// NeoForge versions drop the leading `1.` of the Minecraft version: MC 1.21.1 -> 21.1.x,
/// MC 26.1 -> 26.1.x.
pub fn neoforge_prefix(minecraft: &str) -> String {
    let rest = minecraft.strip_prefix("1.").unwrap_or(minecraft);
    let mut parts: Vec<&str> = rest.split('.').collect();
    if parts.len() == 1 {
        parts.push("0");
    }
    format!("{}.", parts.join("."))
}

/// Loader versions for a Minecraft version, newest first.
pub async fn loader_versions(kind: LoaderKind, minecraft: &str) -> Result<Vec<LoaderVersion>> {
    Ok(match kind {
        LoaderKind::Vanilla => vec![],
        LoaderKind::Fabric => json(&format!("https://meta.fabricmc.net/v2/versions/loader/{minecraft}"))
            .await?
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| {
                Some(LoaderVersion {
                    version: v["loader"]["version"].as_str()?.to_string(),
                    stable: v["loader"]["stable"].as_bool().unwrap_or(true),
                })
            })
            .collect(),
        LoaderKind::Quilt => json(&format!("https://meta.quiltmc.org/v3/versions/loader/{minecraft}"))
            .await?
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| {
                let version = v["loader"]["version"].as_str()?.to_string();
                Some(LoaderVersion { stable: !version.contains('-'), version })
            })
            .collect(),
        LoaderKind::Neoforge => {
            let prefix = neoforge_prefix(minecraft);
            let v = json("https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge").await?;
            let mut out: Vec<LoaderVersion> = v["versions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|s| s.as_str())
                .filter(|s| s.starts_with(&prefix))
                .map(|s| LoaderVersion { version: s.to_string(), stable: !s.contains("beta") && !s.contains("alpha") })
                .collect();
            out.reverse();
            out
        }
        LoaderKind::Forge => {
            let xml = crate::http()
                .get("https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml")
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            let prefix = format!("{minecraft}-");
            xml.split("<version>")
                .skip(1)
                .filter_map(|s| s.split("</version>").next())
                .filter_map(|s| s.strip_prefix(&prefix))
                .map(|s| LoaderVersion { version: s.to_string(), stable: true })
                .collect()
        }
    })
}

/// Required Java major version for a Minecraft version.
pub fn java_for(minecraft: &str) -> u32 {
    let parts: Vec<u32> = minecraft.split(['.', '-', ' ']).map_while(|p| p.parse().ok()).collect();
    match parts.as_slice() {
        [1, minor, rest @ ..] => {
            let patch = rest.first().copied().unwrap_or(0);
            match minor {
                0..=16 => 8,
                17 => 16,
                18 | 19 => 17,
                20 if patch < 5 => 17,
                _ => 21,
            }
        }
        [major, ..] if *major >= 26 => 25,
        _ => 21,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_versions() {
        assert_eq!(java_for("1.12.2"), 8);
        assert_eq!(java_for("1.20.1"), 17);
        assert_eq!(java_for("1.20.6"), 21);
        assert_eq!(java_for("1.21.1"), 21);
        assert_eq!(java_for("26.3"), 25);
    }

    #[test]
    fn neoforge_prefixes() {
        assert_eq!(neoforge_prefix("1.21.1"), "21.1.");
        assert_eq!(neoforge_prefix("1.21"), "21.0.");
        assert_eq!(neoforge_prefix("26.1"), "26.1.");
    }
}
