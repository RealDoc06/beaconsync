//! `beacon-hub`: the self-hosted BeaconSync server.
//!
//! Stores accounts, packs (as content addressed objects + branch refs), releases, game
//! servers and deployments. Server agents long poll it for new targets.

mod auth;
mod db;
mod error;
mod pages;
mod routes;
#[cfg(test)]
mod tests;

use clap::Parser;
use std::{net::SocketAddr, path::PathBuf};

#[derive(Parser, Debug, Clone)]
#[command(version, about = "BeaconSync hub server")]
pub struct Config {
    /// Address to listen on.
    #[arg(long, env = "BEACON_HUB_ADDR", default_value = "0.0.0.0:8787")]
    pub addr: SocketAddr,
    /// Folder for the database and objects.
    #[arg(long, env = "BEACON_HUB_DATA", default_value = "./hub-data")]
    pub data: PathBuf,
    /// Public URL used in share links (e.g. https://hub.example.com).
    #[arg(long, env = "BEACON_HUB_PUBLIC_URL")]
    pub public_url: Option<String>,
    /// Allow anyone to create an account.
    #[arg(long, env = "BEACON_HUB_OPEN_REGISTRATION", default_value_t = true, action = clap::ArgAction::Set)]
    pub open_registration: bool,
    /// Largest object accepted, in megabytes.
    #[arg(long, env = "BEACON_HUB_MAX_OBJECT_MB", default_value_t = 256)]
    pub max_object_mb: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=warn".into()))
        .init();
    let config = Config::parse();
    let state = routes::AppState::open(config.clone())?;
    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind(config.addr).await?;
    tracing::info!("beacon-hub listening on http://{}", config.addr);
    axum::serve(listener, app).with_graceful_shutdown(shutdown()).await?;
    Ok(())
}

async fn shutdown() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("signal handler");
        tokio::select! { _ = ctrl_c => {}, _ = term.recv() => {} }
    }
    #[cfg(not(unix))]
    ctrl_c.await.ok();
}
