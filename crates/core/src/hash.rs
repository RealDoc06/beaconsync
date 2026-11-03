//! SHA-512 helpers. Every object id and file hash in BeaconSync is lowercase hex SHA-512,
//! which also matches the hashes Modrinth publishes for its files.

use sha2::{Digest, Sha512};
use std::{fs::File, io::Read, path::Path};

pub fn sha512(bytes: &[u8]) -> String {
    hex::encode(Sha512::digest(bytes))
}

/// Streams a file through SHA-512 without loading it into memory.
pub fn sha512_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha512::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// True when `s` looks like a full object id.
pub fn is_hash(s: &str) -> bool {
    s.len() == 128 && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// Shortened id for display.
pub fn short(hash: &str) -> &str {
    &hash[..hash.len().min(8)]
}
