//! User settings shared by the desktop app and the CLI (`<config dir>/beaconsync/settings.json`).
//! Set `BEACON_CONFIG_DIR` to use a different folder.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Hub used when the user has not picked one.
pub const DEFAULT_HUB: &str = "http://localhost:8787";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Settings {
    pub hub: Option<String>,
    pub token: Option<String>,
    pub username: Option<String>,
    pub curseforge_key: Option<String>,
    /// Where new instances go. Defaults to Prism's instance folder.
    pub instances_dir: Option<PathBuf>,
    pub prism_path: Option<PathBuf>,
    /// Prism account (profile name) the game starts with. None uses Prism's default.
    pub minecraft_account: Option<String>,
}

impl Settings {
    pub fn dir() -> PathBuf {
        std::env::var_os("BEACON_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("beaconsync"))
    }

    fn path() -> PathBuf {
        Self::dir().join("settings.json")
    }

    pub fn load() -> Self {
        std::fs::read(Self::path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    /// Saves with owner-only permissions since it holds the auth token.
    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        crate::store::write_atomic(&path, &serde_json::to_vec_pretty(self)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    pub fn hub_url(&self) -> String {
        self.hub.clone().unwrap_or_else(|| DEFAULT_HUB.to_string()).trim_end_matches('/').to_string()
    }

    pub fn instances_root(&self) -> PathBuf {
        crate::instance::default_instances_dir(self.instances_dir.as_deref())
    }

    /// Every folder scanned for instances: the configured one, Prism's, and our own.
    pub fn instance_roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![self.instances_root()];
        roots.extend(crate::instance::prism_instances_dir());
        roots.push(crate::instance::data_dir().join("instances"));
        roots.dedup();
        roots
    }
}
