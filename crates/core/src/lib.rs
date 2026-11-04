//! BeaconSync core: the git-like modpack engine shared by the desktop app, the CLI and the hub.
//!
//! - [`model`]: tree/commit/entry types (the on-disk and on-wire format)
//! - [`repo`]: a `.beacon` repository living inside a Prism compatible instance
//! - [`diff`] / [`merge`]: comparing and three-way merging trees
//! - [`sources`]: Modrinth and CurseForge clients
//! - [`import`]: `.mrpack`, CurseForge zips and existing instances
//! - [`server`]: server pack builder, Java runtimes, loader installers
//! - [`hub`]: HTTP client for `beacon-hub`, plus push/fetch of objects

pub mod api;
pub mod companion;
pub mod diff;
pub mod fetch;
pub mod hash;
pub mod hub;
pub mod import;
pub mod instance;
pub mod merge;
pub mod meta;
pub mod model;
pub mod ops;
pub mod repo;
pub mod server;
pub mod settings;
pub mod sources;
pub mod store;

pub use anyhow::{Error, Result};

/// User agent sent to every external API (Modrinth requires a descriptive one).
pub const USER_AGENT: &str = concat!("BeaconSync/", env!("CARGO_PKG_VERSION"), " (github.com/RealDoc06/beaconsync)");

/// Shared HTTP client configured with our user agent.
pub fn http() -> reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .connect_timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("http client")
        })
        .clone()
}

/// Current unix time in seconds.
pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or_default()
}
