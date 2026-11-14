//! Keeps `beacon` itself up to date from BeaconSync's GitHub releases.
//!
//! Every open (except `beacon server ...`, which the hub updates, and debug builds) starts a
//! detached `beacon self-update --background`, which checks at most every `CHECK_EVERY` to stay
//! under GitHub's 60 requests per hour. When a newer release exists it replaces the
//! `beacon` file on disk. Processes already running keep their build; the next start runs the
//! new one. `beacon self-update` does the same in the foreground. The server agent uses
//! `stage` and `Staged::swap` to update itself when the hub asks (server.rs).

use crate::ui;
use anyhow::{Context, Result, bail};
use beacon_core::{
    instance,
    release::{self, Release},
};
use indicatif::{ProgressBar, ProgressStyle};
use std::{fs, path::PathBuf, process::Stdio, time::Duration};

/// Minimum time between background checks. `beacon self-update` always checks.
const CHECK_EVERY: Duration = Duration::from_secs(10 * 60);

/// Starts the background updater without waiting for it. Never fails the command beside it.
pub fn spawn_background() {
    // A dev build would get replaced by the published one.
    if cfg!(debug_assertions) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["self-update", "--background"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // Its own process group, so Ctrl+C in the terminal doesn't cut a download short.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x8;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    if let Err(e) = cmd.spawn() {
        tracing::debug!("starting the update check: {e}");
    }
}

/// `beacon self-update`: installs the newest release over this `beacon`. `background` prints
/// nothing (its output goes nowhere anyway) and skips if the last check was recent.
pub async fn run(background: bool) -> Result<()> {
    if background {
        let dir = instance::data_dir();
        let stamp = dir.join("self-update.checked");
        let recent = fs::metadata(&stamp).and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|e| e < CHECK_EVERY));
        if recent {
            return Ok(());
        }
        fs::create_dir_all(&dir)?;
        fs::write(&stamp, release::VERSION)?;
    }
    let rel = release::latest().await?;
    if !rel.is_newer(release::VERSION) {
        if !background {
            ui::ok(format!("beacon {} is the newest version", release::VERSION));
        }
        return Ok(());
    }
    let bar = if background { ProgressBar::hidden() } else { ProgressBar::new(0) };
    bar.set_style(ProgressStyle::with_template("{bar:30.white/black} {bytes}/{total_bytes} {wide_msg}")?.progress_chars("█▉ "));
    bar.set_message(format!("beacon {}", rel.version()));
    let staged = stage(&rel, |done, total| {
        bar.set_length(total);
        bar.set_position(done);
    })
    .await;
    bar.finish_and_clear();
    staged?.swap()?;
    if !background {
        ui::ok(format!("Installed beacon {}; it runs from the next start", rel.version()));
    }
    Ok(())
}

/// A verified new build next to the running exe, ready to swap in. Holds the update lock, so
/// two updaters never write the same files.
pub struct Staged {
    pub exe: PathBuf,
    new: PathBuf,
    _lock: fs::File,
}

/// Downloads `rel`'s build for this machine next to the running exe and checks that it runs
/// and reports `rel`'s version. `on` gets (bytes so far, total).
pub async fn stage(rel: &Release, on: impl Fn(u64, u64)) -> Result<Staged> {
    let dir = instance::data_dir();
    fs::create_dir_all(&dir)?;
    let lock = fs::File::create(dir.join("self-update.lock"))?;
    if lock.try_lock().is_err() {
        bail!("another beacon update is already running");
    }
    let name = release::cli_asset();
    let asset = rel.asset(&name).with_context(|| format!("{} has no {name}", rel.tag_name))?;
    let exe = std::env::current_exe()?;
    let new = exe.with_file_name(format!("{name}.new"));
    release::download(asset, &new, |done| on(done, asset.size)).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&new, fs::Permissions::from_mode(0o755))?;
    }
    let out = tokio::process::Command::new(&new).arg("--version").output().await.context("running the new beacon")?;
    if !String::from_utf8_lossy(&out.stdout).contains(rel.version()) {
        fs::remove_file(&new).ok();
        bail!("the downloaded beacon does not run on this machine");
    }
    Ok(Staged { exe, new, _lock: lock })
}

impl Staged {
    /// Puts the new build where the running exe is and returns where the old one was kept
    /// (overwritten by the next update). Unix replaces a running file by rename and keeps a
    /// link to the old build; Windows can only rename a running exe, so it moves aside first.
    pub fn swap(self) -> Result<PathBuf> {
        let (exe, new) = (&self.exe, &self.new);
        let old = exe.with_extension(if cfg!(windows) { "old.exe" } else { "old" });
        fs::remove_file(&old).ok();
        #[cfg(unix)]
        {
            fs::hard_link(exe, &old)?;
            fs::rename(new, exe)?;
        }
        #[cfg(not(unix))]
        {
            fs::rename(exe, &old)?;
            if let Err(e) = fs::rename(new, exe) {
                fs::rename(&old, exe).ok();
                return Err(e.into());
            }
        }
        Ok(old)
    }
}
