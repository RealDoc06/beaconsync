//! BeaconSync's own releases on GitHub: what the newest version is and verified downloads of
//! its files. The desktop app updates itself from here and tells servers to update `beacon`.
//!
//! `BEACON_RELEASES` overrides the releases API base (a mirror, a fork, or a local test feed).

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

const API: &str = "https://api.github.com/repos/RealDoc06/beaconsync/releases";
/// How long a download may go without receiving data.
const STALL: Duration = Duration::from_secs(60);

/// The version this build of BeaconSync is.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Release {
    pub tag_name: String,
    /// Release notes (markdown).
    #[serde(default)]
    pub body: Option<String>,
    pub html_url: String,
    pub assets: Vec<Asset>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    /// "sha256:<hex>", computed by GitHub on upload.
    pub digest: Option<String>,
    pub browser_download_url: String,
}

impl Release {
    /// Version without the tag's `v`, e.g. "0.1.0-alpha.2".
    pub fn version(&self) -> &str {
        self.tag_name.trim_start_matches('v')
    }

    /// True when this release is newer than `than`.
    pub fn is_newer(&self, than: &str) -> bool {
        newer(self.version(), than)
    }

    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }
}

fn api() -> String {
    std::env::var("BEACON_RELEASES").unwrap_or_else(|_| API.into())
}

async fn get(url: String) -> Result<Release> {
    let resp = crate::http()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context("checking for BeaconSync updates")?;
    let status = resp.status();
    // Unauthenticated API calls are capped at 60 per hour per IP; GitHub answers 403 with no
    // requests remaining (or 429) once that's used up.
    let limited = status.as_u16() == 429
        || (status.as_u16() == 403 && resp.headers().get("x-ratelimit-remaining").is_some_and(|v| v == "0"));
    ensure!(!limited, "GitHub's update check limit is used up for now, try again later");
    ensure!(status.is_success(), "GitHub answered {status} for {url}");
    Ok(resp.json().await?)
}

/// The newest published release.
pub async fn latest() -> Result<Release> {
    get(format!("{}/latest", api())).await
}

/// A release by version ("0.1.0-alpha.2") or tag ("v0.1.0-alpha.2").
pub async fn by_version(version: &str) -> Result<Release> {
    get(format!("{}/tags/v{}", api(), version.trim_start_matches('v'))).await
}

/// Downloads `asset` to `dest` through a temporary file next to it, checking its size and
/// GitHub's sha256. `dest` only ever appears complete and verified. `on` gets bytes so far.
pub async fn download(asset: &Asset, dest: &Path, on: impl Fn(u64)) -> Result<()> {
    let want = asset.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).context("the release file has no checksum")?;
    let mut resp = crate::http().get(&asset.browser_download_url).send().await?.error_for_status()?;
    let dir = dest.parent().context("no download folder")?;
    std::fs::create_dir_all(dir)?;
    let part = PathBuf::from(format!("{}.part", dest.display()));
    let mut file = std::fs::File::create(&part)?;
    let (mut hash, mut done) = (Sha256::new(), 0u64);
    // A stalled connection fails instead of hanging the app or the server agent.
    while let Some(chunk) = tokio::time::timeout(STALL, resp.chunk()).await.context("the download stalled")?? {
        done += chunk.len() as u64;
        if done > asset.size {
            break;
        }
        hash.update(&chunk);
        file.write_all(&chunk)?;
        on(done);
    }
    file.sync_all()?;
    drop(file);
    if done != asset.size || hex::encode(hash.finalize()) != want {
        std::fs::remove_file(&part).ok();
        bail!("{} failed verification (size or checksum mismatch)", asset.name);
    }
    std::fs::rename(&part, dest)?;
    Ok(())
}

/// Semver order for `MAJOR.MINOR.PATCH[-PRE]`: a prerelease sorts before its release and
/// numeric prerelease parts compare as numbers ("alpha.10" > "alpha.9").
pub fn newer(a: &str, b: &str) -> bool {
    key(a) > key(b)
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Num(u64),
    Text(String),
}

fn key(v: &str) -> (Vec<u64>, bool, Vec<Part>) {
    let v = v.trim_start_matches('v').split('+').next().unwrap_or_default();
    let (core, pre) = v.split_once('-').map_or((v, None), |(c, p)| (c, Some(p)));
    let core = core.split('.').map(|n| n.parse().unwrap_or(0)).collect();
    let pre_parts =
        pre.unwrap_or_default().split('.').map(|p| p.parse().map_or_else(|_| Part::Text(p.into()), Part::Num)).collect();
    // `true` (no prerelease) sorts after `false`.
    (core, pre.is_none(), pre_parts)
}

/// Name of the `beacon` CLI file built for this machine, e.g. "beacon-x86_64-unknown-linux-musl".
pub fn cli_asset() -> String {
    let os = match std::env::consts::OS {
        "macos" => "apple-darwin",
        "windows" => "pc-windows-msvc.exe",
        _ => "unknown-linux-musl",
    };
    format!("beacon-{}-{os}", std::env::consts::ARCH)
}

#[cfg(test)]
mod tests {
    use super::newer;

    #[test]
    fn orders_versions_like_semver() {
        assert!(newer("0.1.0-alpha.2", "0.1.0-alpha.1"));
        assert!(newer("0.1.0-alpha.10", "0.1.0-alpha.9"));
        assert!(newer("0.1.0", "0.1.0-alpha.9"));
        assert!(newer("v0.2.0", "0.1.9"));
        assert!(newer("0.1.0-beta.1", "0.1.0-alpha.3"));
        assert!(!newer("0.1.0-alpha.1", "0.1.0-alpha.1"));
        assert!(!newer("0.1.0-alpha.1", "0.1.0"));
    }
}
