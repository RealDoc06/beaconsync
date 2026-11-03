//! Content addressed object store: `<root>/ab/cdef...` where the name is the SHA-512 of the bytes.

use crate::hash::{self, sha512};
use anyhow::{Context, Result, bail};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct ObjectStore {
    root: PathBuf,
}

impl ObjectStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn path(&self, hash: &str) -> PathBuf {
        self.root.join(&hash[..2]).join(&hash[2..])
    }

    pub fn has(&self, hash: &str) -> bool {
        hash::is_hash(hash) && self.path(hash).is_file()
    }

    pub fn get(&self, hash: &str) -> Result<Vec<u8>> {
        if !hash::is_hash(hash) {
            bail!("invalid object id {hash}");
        }
        fs::read(self.path(hash)).with_context(|| format!("missing object {}", hash::short(hash)))
    }

    pub fn get_json<T: DeserializeOwned>(&self, hash: &str) -> Result<T> {
        Ok(serde_json::from_slice(&self.get(hash)?)?)
    }

    /// Stores bytes and returns their id. No-op when the object already exists.
    pub fn put(&self, bytes: &[u8]) -> Result<String> {
        let id = sha512(bytes);
        self.put_verified(&id, bytes)?;
        Ok(id)
    }

    /// Stores bytes under a known id after checking the hash matches.
    pub fn put_verified(&self, id: &str, bytes: &[u8]) -> Result<()> {
        if self.has(id) {
            return Ok(());
        }
        if sha512(bytes) != id {
            bail!("hash mismatch for object {}", hash::short(id));
        }
        write_atomic(&self.path(id), bytes)
    }

    pub fn put_json<T: Serialize>(&self, value: &T) -> Result<String> {
        self.put(&serde_json::to_vec(value)?)
    }

    /// Copies a file into the store when its hash is already known.
    pub fn put_file(&self, id: &str, file: &Path) -> Result<()> {
        if self.has(id) {
            return Ok(());
        }
        let bytes = fs::read(file).with_context(|| format!("reading {}", file.display()))?;
        self.put_verified(id, &bytes)
    }
}

/// Writes through a temp file in the same directory, then renames, so readers never see
/// half written files.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().context("path has no parent")?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.tmp{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("obj"),
        std::process::id()
    ));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, path)?;
    Ok(())
}
