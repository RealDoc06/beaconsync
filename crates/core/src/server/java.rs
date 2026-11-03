//! Finds a matching Java runtime, downloading Eclipse Temurin when none is installed.

use crate::fetch::{OnProgress, Progress};
use anyhow::{Context, Result, bail};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Major version of a `java` executable (8 for "1.8.0_x").
pub fn major(java: &Path) -> Option<u32> {
    let out = Command::new(java).arg("-version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stderr).to_string() + &String::from_utf8_lossy(&out.stdout);
    let v = text.split('"').nth(1)?;
    let mut parts = v.split(['.', '_', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 { parts.next()?.parse().ok() } else { Some(first) }
}

fn exe() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

fn runtimes_dir() -> PathBuf {
    crate::instance::data_dir().join("java")
}

fn find_in(dir: &Path) -> Option<PathBuf> {
    walkdir::WalkDir::new(dir)
        .max_depth(5)
        .into_iter()
        .flatten()
        .find(|e| e.file_name() == exe() && e.path().parent().is_some_and(|p| p.ends_with("bin")))
        .map(|e| e.into_path())
}

/// Old Minecraft (Java 8) needs exactly 8; newer versions run on the required major or later.
fn compatible(found: u32, wanted: u32) -> bool {
    if wanted <= 8 { found == 8 } else { found >= wanted && found <= wanted + 4 }
}

/// Returns a Java executable for `wanted`, downloading one into BeaconSync's data dir if needed.
pub async fn ensure(wanted: u32, on: OnProgress<'_>) -> Result<PathBuf> {
    let mut candidates: Vec<PathBuf> = vec![];
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        candidates.push(PathBuf::from(home).join("bin").join(exe()));
    }
    candidates.push(PathBuf::from(exe()));
    if let Some(p) = find_in(&runtimes_dir().join(wanted.to_string())) {
        candidates.insert(0, p);
    }
    for c in candidates {
        if major(&c).is_some_and(|m| compatible(m, wanted)) {
            return Ok(c);
        }
    }
    download(wanted, on).await
}

async fn download(major_version: u32, on: OnProgress<'_>) -> Result<PathBuf> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "mac",
        "windows" => "windows",
        o => bail!("no Java download for {o}"),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        a => bail!("no Java download for {a}"),
    };
    on(Progress { done: 0, total: 1, label: format!("Downloading Java {major_version}") });
    let url = format!("https://api.adoptium.net/v3/binary/latest/{major_version}/ga/{os}/{arch}/jre/hotspot/normal/eclipse");
    let bytes = crate::http().get(&url).send().await?.error_for_status()?.bytes().await?;
    let dest = runtimes_dir().join(major_version.to_string());
    std::fs::remove_dir_all(&dest).ok();
    std::fs::create_dir_all(&dest)?;
    let d = dest.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        if os == "windows" {
            zip::ZipArchive::new(std::io::Cursor::new(bytes))?.extract(&d)?;
        } else {
            tar::Archive::new(flate2::read::GzDecoder::new(std::io::Cursor::new(bytes))).unpack(&d)?;
        }
        Ok(())
    })
    .await??;
    on(Progress { done: 1, total: 1, label: format!("Java {major_version} ready") });
    find_in(&dest).context("downloaded Java runtime has no java executable")
}
