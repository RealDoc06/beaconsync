//! Materializes tree entries on disk: from the local object store, the entry's download
//! source, or a hub. Every download is SHA-512 verified before it lands.

use crate::{hash, model::Entry, store::ObjectStore};
use anyhow::{Context, Result, bail};
use futures::{StreamExt, TryStreamExt, stream};
use serde::Serialize;
use sha2::{Digest, Sha512};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::io::AsyncWriteExt;

#[derive(Serialize, Clone, Debug)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub label: String,
}

/// Progress callback shared by long running operations.
pub type OnProgress<'a> = &'a (dyn Fn(Progress) + Send + Sync);

pub fn no_progress(_: Progress) {}

#[derive(Clone, Debug, Default)]
pub struct Fetcher {
    pub store: Option<ObjectStore>,
    /// Hub base url used for blobs that have no download source.
    pub hub: Option<String>,
}

impl Fetcher {
    pub fn new(store: Option<ObjectStore>, hub: Option<String>) -> Self {
        Self { store, hub }
    }

    /// Writes `entry` to `dest` (atomically).
    pub async fn fetch_to(&self, entry: &Entry, dest: &Path) -> Result<()> {
        if let Some(store) = &self.store
            && store.has(&entry.hash)
        {
            let bytes = store.get(&entry.hash)?;
            return crate::store::write_atomic(dest, &bytes);
        }
        if let Some(src) = &entry.source {
            match download(src.url(), &entry.hash, dest).await {
                Ok(()) => return Ok(()),
                Err(e) if self.hub.is_none() => return Err(e),
                Err(e) => tracing::warn!("source download failed, trying hub: {e:#}"),
            }
        }
        let Some(hub) = &self.hub else {
            bail!("object {} is not available locally and no hub is configured", hash::short(&entry.hash));
        };
        download(&format!("{hub}/api/objects/{}", entry.hash), &entry.hash, dest).await?;
        if entry.source.is_none()
            && let Some(store) = &self.store
        {
            store.put_file(&entry.hash, dest).ok();
        }
        Ok(())
    }

    /// Fetches many entries with bounded parallelism.
    pub async fn fetch_many(&self, items: Vec<(Entry, PathBuf)>, on: OnProgress<'_>) -> Result<()> {
        let total = items.len();
        let done = Arc::new(AtomicUsize::new(0));
        stream::iter(items)
            .map(|(entry, dest)| {
                let done = done.clone();
                async move {
                    self.fetch_to(&entry, &dest).await.with_context(|| format!("downloading {}", dest.display()))?;
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    let label = entry
                        .name
                        .clone()
                        .or_else(|| dest.file_name().map(|f| f.to_string_lossy().into_owned()))
                        .unwrap_or_default();
                    on(Progress { done: n, total, label });
                    anyhow::Ok(())
                }
            })
            .buffer_unordered(8)
            .try_collect::<()>()
            .await
    }
}

/// Streams `url` to `dest`, verifying the SHA-512 matches `expected`.
pub async fn download(url: &str, expected: &str, dest: &Path) -> Result<()> {
    let resp = crate::http().get(url).send().await?.error_for_status()?;
    let dir = dest.parent().context("no parent")?;
    tokio::fs::create_dir_all(dir).await?;
    let tmp = dir.join(format!(".{}.part", dest.file_name().unwrap_or_default().to_string_lossy()));
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut hasher = Sha512::new();
    let mut body = resp.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk?;
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    let got = hex::encode(hasher.finalize());
    if got != expected {
        tokio::fs::remove_file(&tmp).await.ok();
        bail!("hash mismatch downloading {url}");
    }
    tokio::fs::rename(&tmp, dest).await?;
    Ok(())
}

/// Downloads `url` to `dest` without a known hash and returns the SHA-512.
pub async fn download_unverified(url: &str, dest: &Path) -> Result<String> {
    let bytes = crate::http().get(url).send().await?.error_for_status()?.bytes().await?;
    crate::store::write_atomic(dest, &bytes)?;
    Ok(hash::sha512(&bytes))
}
