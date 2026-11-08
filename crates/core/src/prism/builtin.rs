//! BeaconSync's own portable Prism, for computers without one installed. Its program lives in
//! `<data>/prism/<version>/` and its data (accounts, settings, game files) in `<data>/prism/data/`.

use crate::{
    fetch::{OnProgress, Progress},
    instance::{data_dir, ini_set},
    release::{self, Asset},
    store::write_atomic,
};
use anyhow::{Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

/// Prism version BeaconSync downloads. On a bump, update every checksum and size in [`build`].
const VERSION: &str = "11.1.1";

/// A portable Prism archive on Prism's release page and the executable inside it.
struct Build {
    /// Middle of the file name: `PrismLauncher-<part>-<VERSION>.<ext>`.
    part: &'static str,
    ext: &'static str,
    size: u64,
    sha256: &'static str,
    exe: &'static str,
}

impl Build {
    fn file(&self) -> String {
        format!("PrismLauncher-{}-{VERSION}.{}", self.part, self.ext)
    }
}

/// The portable build for this computer, if Prism makes one.
fn build() -> Option<Build> {
    let b = |part, ext, size, sha256, exe| Build { part, ext, size, sha256, exe };
    let linux = "PrismLauncher";
    let windows = "prismlauncher.exe";
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => b(
            "Linux-Qt6-Portable",
            "tar.gz",
            63_960_101,
            "e21949a8101c1803dbc6f67a2cdad48ba80ede1c133f0d1f2a89da07af09e823",
            linux,
        ),
        ("linux", "aarch64") => b(
            "Linux-aarch64-Qt6-Portable",
            "tar.gz",
            62_833_681,
            "d115b5699bba71e74512bd6c8ed7e2682ba189939d1e59b405a5b7b46360720a",
            linux,
        ),
        ("macos", _) => b(
            "macOS",
            "zip",
            43_614_882,
            "15126a7df57acb4f37fe581dad68cb03c7bb60b70eb82345f8bb3246235600c6",
            "Prism Launcher.app/Contents/MacOS/prismlauncher",
        ),
        ("windows", "x86_64") => b(
            "Windows-MSVC-Portable",
            "zip",
            20_396_629,
            "ab35a770fb06d89d2ccc098079db5db329fb4e68f42b72babd8b095efde3d2d7",
            windows,
        ),
        ("windows", "aarch64") => b(
            "Windows-MSVC-arm64-Portable",
            "zip",
            18_629_802,
            "f33b34a6f313fc46c1d5912256942d50216814577ad2ff9dfc28102de7208e53",
            windows,
        ),
        _ => return None,
    })
}

/// True when Prism makes a portable build for this computer.
pub fn available() -> bool {
    build().is_some()
}

/// `<data>/prism/`: one folder per Prism version, plus `data/`.
pub fn root() -> PathBuf {
    data_dir().join("prism")
}

/// The built-in Prism's data folder (accounts, settings, game files).
pub fn data() -> PathBuf {
    root().join("data")
}

/// The built-in Prism's executable, downloaded first if needed.
pub async fn ensure(on: OnProgress<'_>) -> Result<PathBuf> {
    // Two clicks on Play must not download into the same files.
    static DOWNLOAD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _one_at_a_time = DOWNLOAD.lock().await;
    let build = build().context("There is no Prism Launcher build for this computer. Install it from prismlauncher.org")?;
    let app = root().join(VERSION);
    if !app.join(build.exe).is_file() {
        download(&build, &app, on).await?;
    }
    Ok(app.join(build.exe))
}

/// Runs `exe` on the data folder `data`, creating its settings on first use. `instances` is
/// where it finds instances.
pub fn command(exe: &Path, data: &Path, instances: &Path) -> Result<Command> {
    fs::create_dir_all(data)?;
    let cfg = data.join("prismlauncher.cfg");
    let text = fs::read_to_string(&cfg).unwrap_or_else(|_| FIRST_RUN.into());
    // Forward slashes and quotes keep Qt from reading backslashes as escapes or commas as lists.
    let dir = format!("\"{}\"", instances.display().to_string().replace('\\', "/"));
    // Kept on every run, not just the first: BeaconSync reports crashes instead of Prism.
    let updated = ini_set(&ini_set(&text, "InstanceDir", &dir), "ShowConsoleOnError", "false");
    if updated != text {
        write_atomic(&cfg, updated.as_bytes())?;
    }
    let mut cmd = Command::new(exe);
    // The Linux start script turns on portable mode when `portable.txt` is in the working dir.
    cmd.arg("--dir").arg(data).current_dir(data);
    Ok(cmd)
}

/// Downloads and unpacks `build` into `app`, removing older Prism versions.
async fn download(build: &Build, app: &Path, on: OnProgress<'_>) -> Result<()> {
    let file = build.file();
    let asset = Asset {
        browser_download_url: format!("https://github.com/PrismLauncher/PrismLauncher/releases/download/{VERSION}/{file}"),
        size: build.size,
        digest: Some(format!("sha256:{}", build.sha256)),
        name: file.clone(),
    };
    let archive = root().join(&file);
    let (total, last) = ((build.size / 1_000_000) as usize, AtomicUsize::new(usize::MAX));
    let label = "Downloading Prism Launcher (MB)";
    release::download(&asset, &archive, |bytes| {
        let mb = (bytes / 1_000_000) as usize;
        if last.swap(mb, Ordering::Relaxed) != mb {
            on(Progress { done: mb, total, label: label.into() });
        }
    })
    .await?;
    on(Progress { done: 0, total: 0, label: "Unpacking Prism Launcher".into() });
    let staging = root().join(format!("{VERSION}.part"));
    fs::remove_dir_all(&staging).ok();
    fs::create_dir_all(&staging)?;
    let (from, to, ext) = (archive.clone(), staging.clone(), build.ext);
    tokio::task::spawn_blocking(move || unpack(&from, &to, ext)).await??;
    fs::remove_file(&archive).ok();
    for old in fs::read_dir(root())?.flatten() {
        if old.file_name() != "data" && old.file_name() != "sign-in" && old.path() != staging {
            fs::remove_dir_all(old.path()).ok();
        }
    }
    // A half unpacked folder from an older attempt would block the rename on Windows.
    fs::remove_dir_all(app).ok();
    fs::rename(&staging, app)?;
    Ok(())
}

fn unpack(archive: &Path, dest: &Path, ext: &str) -> Result<()> {
    if ext == "tar.gz" {
        tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(archive)?)).unpack(dest)?;
    } else if cfg!(target_os = "macos") {
        // ditto keeps the app bundle's symlinks and signature intact.
        let ok = Command::new("ditto").arg("-x").arg("-k").arg(archive).arg(dest).status()?.success();
        anyhow::ensure!(ok, "could not unpack Prism Launcher");
    } else {
        zip::ZipArchive::new(fs::File::open(archive)?)?.extract(dest)?;
    }
    Ok(())
}

/// Settings for a fresh built-in Prism. They answer its setup questions (language, theme, Java),
/// so its first run only asks the player to sign in with Microsoft.
const FIRST_RUN: &str = "[General]
ConfigVersion=1.3
Language=en_US
ApplicationTheme=system
IconTheme=pe_colored
AutomaticJavaDownload=true
AutomaticJavaSwitch=true
";
