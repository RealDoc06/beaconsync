//! The in-game companion mod. Installed into joined Fabric/Quilt packs so players get update
//! toasts. It is ignored by versioning (see `repo::DEFAULT_IGNORE`), so it never shows up as a
//! change or ships to servers.

use crate::{model::LoaderKind, repo::Repo};
use anyhow::Result;

/// Minecraft versions the companion mod is built for.
pub const SUPPORTED: &[&str] = &["1.21.1", "26.3"];

pub const RELEASES: &str = "https://github.com/RealDoc06/beaconsync/releases/latest/download";

/// Downloads the companion into `mods/` when the pack's game version is supported.
/// Returns false when there is no build for this pack.
pub async fn install(repo: &Repo) -> Result<bool> {
    let tree = repo.scan()?;
    let fabric_like = matches!(tree.loader.kind, LoaderKind::Fabric | LoaderKind::Quilt);
    if !fabric_like || !SUPPORTED.contains(&tree.minecraft.as_str()) {
        return Ok(false);
    }
    let name = format!("beaconsync-companion-{}.jar", tree.minecraft);
    let resp = crate::http().get(format!("{RELEASES}/{name}")).send().await?;
    if !resp.status().is_success() {
        return Ok(false);
    }
    let bytes = resp.bytes().await?;
    crate::store::write_atomic(&repo.game_dir().join("mods").join(name), &bytes)?;
    Ok(true)
}
