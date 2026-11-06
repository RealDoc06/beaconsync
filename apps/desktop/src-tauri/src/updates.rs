//! The app updating itself from BeaconSync's GitHub releases, the way Musicah does it: a
//! background check finds a newer release, the first click downloads the installer for this
//! kind of install (verified against GitHub's sha256), the second installs it and restarts.
//! Installs it can't update in place (dev builds, unknown packages) only link to the release.
//!
//! The UI reads [`UpdateView`] with `update_status` and gets each change as an `update` event.

use anyhow::Context;
use beacon_core::release::{self, Asset, Release, VERSION};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::util::Res;

const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Available,
    /// Newer version out, but this install can't apply it (opens the release page instead).
    External,
    Downloading,
    Verifying,
    Ready,
    Installing,
    Failed,
}

#[derive(Serialize, Clone, Debug)]
pub struct Offer {
    pub version: String,
    pub notes: String,
    pub url: String,
    pub phase: Phase,
    pub downloaded: u64,
    pub total: u64,
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct UpdateView {
    pub current: &'static str,
    /// Newest release, also the version servers are offered.
    pub latest: Option<String>,
    /// Set when `latest` is newer than this app.
    pub offer: Option<Offer>,
}

/// How this copy of the app was installed, which decides the file to download and how to apply it.
#[derive(Clone, Debug)]
enum Kind {
    /// Tauri's NSIS setup.exe (per user install).
    Nsis,
    /// MSI, relaunched from this exe afterwards.
    Msi(PathBuf),
    /// `.app` bundle from the dmg.
    Dmg(PathBuf),
    AppImage(PathBuf),
    /// deb/rpm packages put the binary here; `pkexec` asks for the password.
    Deb(PathBuf),
    Rpm(PathBuf),
    External,
}

impl Kind {
    fn detect() -> Self {
        if cfg!(debug_assertions) {
            return Self::External;
        }
        let Ok(exe) = std::env::current_exe() else { return Self::External };
        if cfg!(target_os = "linux") {
            if let Some(image) = std::env::var_os("APPIMAGE") {
                return Self::AppImage(image.into());
            }
            let tool = |p: &str| Path::new(p).is_file();
            if exe.starts_with("/usr/bin") && tool("/usr/bin/pkexec") {
                if tool("/usr/bin/dpkg") {
                    return Self::Deb(exe);
                }
                if tool("/usr/bin/rpm") {
                    return Self::Rpm(exe);
                }
            }
        }
        if cfg!(target_os = "macos")
            && let Some(bundle) = exe.ancestors().nth(3).filter(|b| b.extension().is_some_and(|e| e == "app"))
        {
            return Self::Dmg(bundle.to_path_buf());
        }
        if cfg!(windows) {
            // NSIS installs per user, the MSI under Program Files. Anything else is a loose exe.
            let under = |var: &str| std::env::var_os(var).is_some_and(|d| exe.starts_with(d));
            if under("LOCALAPPDATA") {
                return Self::Nsis;
            }
            if under("ProgramFiles") {
                return Self::Msi(exe);
            }
        }
        Self::External
    }

    /// The release file for this install, matched by the names tauri-action gives them.
    fn asset<'a>(&self, r: &'a Release) -> Option<&'a Asset> {
        let suffix = match self {
            Self::Nsis => "-setup.exe",
            Self::Msi(_) => ".msi",
            Self::Dmg(_) => ".dmg",
            Self::AppImage(_) => ".AppImage",
            Self::Deb(_) => ".deb",
            Self::Rpm(_) => ".rpm",
            Self::External => return None,
        };
        let arch: &[&str] = match std::env::consts::ARCH {
            "aarch64" => &["aarch64", "arm64", "universal"],
            _ => &["x86_64", "amd64", "x64", "universal"],
        };
        r.assets.iter().find(|a| a.name.ends_with(suffix) && arch.iter().any(|x| a.name.contains(x)))
    }
}

struct Inner {
    latest: Option<Release>,
    phase: Phase,
    downloaded: u64,
    error: Option<String>,
    file: Option<PathBuf>,
}

/// App state for updates; managed by Tauri.
pub struct Updates {
    kind: Kind,
    inner: Mutex<Inner>,
}

impl Updates {
    fn view(&self) -> UpdateView {
        let s = self.inner.lock().unwrap();
        let offer = s.latest.as_ref().filter(|r| r.is_newer(VERSION)).map(|r| Offer {
            version: r.version().to_string(),
            notes: r.body.clone().unwrap_or_default(),
            url: r.html_url.clone(),
            phase: if s.phase == Phase::Available && self.kind.asset(r).is_none() { Phase::External } else { s.phase },
            downloaded: s.downloaded,
            total: self.kind.asset(r).map_or(0, |a| a.size),
            error: s.error.clone(),
        });
        UpdateView { current: VERSION, latest: s.latest.as_ref().map(|r| r.version().to_string()), offer }
    }

    fn update(&self, app: &AppHandle, f: impl FnOnce(&mut Inner)) {
        f(&mut self.inner.lock().unwrap());
        app.emit("update", self.view()).ok();
    }

    /// Moves to `to` when `f` allows it, checked and set under one lock so two clicks can't
    /// both start. `f` returns `None` when there's nothing to do.
    fn claim<T>(
        &self,
        app: &AppHandle,
        to: Phase,
        f: impl FnOnce(&Inner) -> anyhow::Result<Option<T>>,
    ) -> anyhow::Result<Option<T>> {
        let out = {
            let mut s = self.inner.lock().unwrap();
            let out = f(&s)?;
            if out.is_some() {
                (s.phase, s.downloaded, s.error) = (to, 0, None);
            }
            out
        };
        app.emit("update", self.view()).ok();
        Ok(out)
    }
}

/// Registers the state and starts the background check.
pub fn start(app: &AppHandle) {
    app.manage(Updates {
        kind: Kind::detect(),
        inner: Mutex::new(Inner { latest: None, phase: Phase::Available, downloaded: 0, error: None, file: None }),
    });
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            if let Ok(r) = release::latest().await {
                let u = app.state::<Updates>();
                let changed = u.inner.lock().unwrap().latest.as_ref().is_none_or(|old| old.tag_name != r.tag_name);
                if changed {
                    u.update(&app, |s| {
                        *s = Inner { latest: Some(r), phase: Phase::Available, downloaded: 0, error: None, file: None };
                    });
                }
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

#[tauri::command]
pub fn update_status(u: State<'_, Updates>) -> UpdateView {
    u.view()
}

/// Downloads and verifies the installer for the offered version. Progress arrives as `update` events.
#[tauri::command]
pub async fn update_download(app: AppHandle, u: State<'_, Updates>) -> Res<()> {
    let dir = app.path().app_cache_dir()?.join("updates");
    let started = u.claim(&app, Phase::Downloading, |s| {
        let r = s.latest.clone().filter(|r| r.is_newer(VERSION)).context("no update available")?;
        let a = u.kind.asset(&r).cloned().context("this install can't update itself")?;
        let busy = matches!(s.phase, Phase::Downloading | Phase::Verifying | Phase::Ready | Phase::Installing);
        Ok((!busy).then_some((r.tag_name, a)))
    })?;
    let Some((tag, asset)) = started else { return Ok(()) };
    // Only while the offer is still this release: the background check may find a newer one.
    let set = |f: &dyn Fn(&mut Inner)| {
        u.update(&app, |s| {
            if s.latest.as_ref().is_some_and(|r| r.tag_name == tag) {
                f(s)
            }
        })
    };
    let dest = dir.join(&asset.name);
    let percent = Mutex::new(0);
    let result = release::download(&asset, &dest, |done| {
        // At most one event per percent.
        let p = done * 100 / asset.size.max(1);
        let mut last = percent.lock().unwrap();
        if p != *last {
            *last = p;
            let phase = if done == asset.size { Phase::Verifying } else { Phase::Downloading };
            set(&|s| (s.phase, s.downloaded) = (phase, done));
        }
    })
    .await;
    match result {
        Ok(()) => set(&|s| (s.phase, s.file) = (Phase::Ready, Some(dest.clone()))),
        Err(e) => {
            set(&|s| (s.phase, s.error) = (Phase::Failed, Some(format!("{e:#}"))));
            return Err(e.into());
        }
    }
    Ok(())
}

/// Installs the downloaded update and restarts into it.
#[tauri::command]
pub async fn update_install(app: AppHandle, u: State<'_, Updates>) -> Res<()> {
    let file = u
        .claim(&app, Phase::Installing, |s| Ok(s.file.clone().filter(|_| s.phase == Phase::Ready)))?
        .context("the update isn't downloaded")?;
    let kind = u.kind.clone();
    let installed = tauri::async_runtime::spawn_blocking(move || install(&kind, &file)).await;
    match installed.map_err(anyhow::Error::from).and_then(|r| Ok(r?.spawn()?)) {
        Ok(_) => {
            app.exit(0);
            Ok(())
        }
        Err(e) => {
            // Back to ready: the file is still good, the user can try again.
            u.update(&app, |s| (s.phase, s.error) = (Phase::Ready, Some(format!("{e:#}"))));
            Err(e.into())
        }
    }
}

/// Applies `file` and returns the command that starts the updated app (or the installer that
/// does). The caller runs it and quits.
fn install(kind: &Kind, file: &Path) -> anyhow::Result<Command> {
    use anyhow::{bail, ensure};
    let run = |c: &mut Command| -> anyhow::Result<()> {
        let out = c.output().with_context(|| format!("running {c:?}"))?;
        ensure!(out.status.success(), "{c:?} failed: {}", String::from_utf8_lossy(&out.stderr).trim());
        Ok(())
    };
    Ok(match kind {
        Kind::AppImage(image) => {
            let staged = image.with_extension("AppImage.new");
            std::fs::copy(file, &staged)?;
            #[cfg(unix)]
            std::fs::set_permissions(&staged, std::os::unix::fs::PermissionsExt::from_mode(0o755))?;
            std::fs::rename(&staged, image)?;
            Command::new(image)
        }
        Kind::Deb(exe) => {
            run(Command::new("/usr/bin/pkexec").args(["/usr/bin/dpkg", "-i"]).arg(file))?;
            Command::new(exe)
        }
        Kind::Rpm(exe) => {
            run(Command::new("/usr/bin/pkexec").args(["/usr/bin/rpm", "-U", "--replacepkgs"]).arg(file))?;
            Command::new(exe)
        }
        Kind::Dmg(bundle) => {
            let mount = file.with_extension("mount");
            std::fs::create_dir_all(&mount)?;
            run(Command::new("/usr/bin/hdiutil")
                .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
                .arg(&mount)
                .arg(file))?;
            let staged = bundle.with_extension("app.new");
            let copied = std::fs::read_dir(&mount)?
                .flatten()
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|e| e == "app"))
                .context("no app in the dmg")
                .and_then(|app| {
                    std::fs::remove_dir_all(&staged).ok();
                    run(Command::new("/usr/bin/ditto").arg(app).arg(&staged))
                });
            run(Command::new("/usr/bin/hdiutil").arg("detach").arg(&mount)).ok();
            copied?;
            let old = bundle.with_extension("app.old");
            std::fs::remove_dir_all(&old).ok();
            std::fs::rename(bundle, &old)?;
            if let Err(e) = std::fs::rename(&staged, bundle) {
                std::fs::rename(&old, bundle).ok();
                bail!(e);
            }
            std::fs::remove_dir_all(&old).ok();
            let mut open = Command::new("/usr/bin/open");
            open.arg("-n").arg(bundle);
            open
        }
        // The installer closes this app, installs and starts the new version (/R).
        Kind::Nsis => {
            let mut c = Command::new(file);
            c.args(["/P", "/R", "/UPDATE"]);
            c
        }
        Kind::Msi(exe) => {
            let mut c = Command::new("cmd");
            c.arg("/C");
            #[cfg(windows)]
            std::os::windows::process::CommandExt::raw_arg(
                &mut c,
                format!("msiexec /i \"{}\" /passive /norestart & start \"\" \"{}\"", file.display(), exe.display()),
            );
            #[cfg(not(windows))]
            let _ = exe;
            c
        }
        Kind::External => bail!("this install can't update itself"),
    })
}
