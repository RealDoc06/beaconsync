//! Shared plumbing for commands: error type, progress events, blocking helper.

use beacon_core::{fetch::Progress, repo::Repo, settings::Settings};
use serde::{Serialize, Serializer};
use std::path::Path;
use tauri::{AppHandle, Emitter};

/// Command error. Serializes to the full anyhow chain as a plain string, so the frontend
/// receives `Result<T, string>`. Any error type converts with `?`.
#[derive(Debug)]
pub struct Error(String);

impl<E: Into<anyhow::Error>> From<E> for Error {
    fn from(e: E) -> Self {
        Self(format!("{:#}", e.into()))
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

pub type Res<T> = Result<T, Error>;

/// Runs sync, disk heavy core work (scan, status, zip) off the async runtime.
pub async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> Res<T> {
    Ok(tauri::async_runtime::spawn_blocking(f).await??)
}

/// Progress callback for long operations; the UI listens to the `progress` event.
pub fn progress(app: &AppHandle) -> impl Fn(Progress) + Send + Sync + 'static {
    let app = app.clone();
    move |p| {
        app.emit("progress", p).ok();
    }
}

pub fn open(path: &str) -> anyhow::Result<Repo> {
    Repo::open(Path::new(path))
}

/// Name recorded on saved versions: hub username, else the OS user.
pub fn author(s: &Settings) -> String {
    s.username
        .clone()
        .or_else(|| std::env::var("USER").or_else(|_| std::env::var("USERNAME")).ok())
        .unwrap_or_else(|| "me".into())
}
