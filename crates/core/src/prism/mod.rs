//! Prism Launcher plays the game: it handles Microsoft accounts, Java and game files.
//!
//! An installed Prism is used when there is one, so players keep their accounts and instances.
//! Otherwise BeaconSync downloads a pinned portable Prism ([`builtin`]) the first time someone
//! plays. Players never see its window: `--launch` starts the game without it (Prism quits with
//! the game), accounts are added and removed from BeaconSync ([`accounts`]) and crashes are
//! reported by BeaconSync ([`Session::wait`]).

pub mod accounts;
mod builtin;

pub use accounts::Account;

use crate::{
    fetch::OnProgress,
    instance::{game_dir, prism_data_dir, prism_instances_dir},
    settings::Settings,
};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::SystemTime,
};

/// Which Prism plays the game on this computer (shown in Settings).
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// The player's own Prism (set in Settings or found on this machine).
    Installed,
    /// BeaconSync's own copy, downloaded on first play.
    BuiltIn,
    /// Neither: no installed Prism and no portable build for this OS/CPU.
    Missing,
}

pub fn kind(s: &Settings) -> Kind {
    if !uses_builtin(s) {
        Kind::Installed
    } else if builtin::available() {
        Kind::BuiltIn
    } else {
        Kind::Missing
    }
}

/// True when no Prism is installed, so the built-in one plays the game.
fn uses_builtin(s: &Settings) -> bool {
    installed(s.prism_path.as_deref()).is_none()
}

/// Prism processes started by this BeaconSync process that are still running. Prism rewrites
/// its files while running, so BeaconSync leaves them alone until this is zero. A game
/// started from another BeaconSync process (the CLI) is not counted.
static RUNNING: AtomicUsize = AtomicUsize::new(0);

/// True while a Prism started here is running or about to (Play pressed, Prism downloading).
pub fn busy() -> bool {
    RUNNING.load(Ordering::SeqCst) > 0
}

/// Counts toward [`busy`] from the moment Play is pressed until Prism exits.
struct Running;

impl Running {
    fn new() -> Self {
        RUNNING.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        RUNNING.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Set while [`accounts::sign_in`] runs. Play waits for it: a Prism started meanwhile would
/// load the account list before the new account lands and later save over it.
static SIGNING_IN: AtomicBool = AtomicBool::new(false);

/// How a game started by [`launch`] went wrong.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Failure {
    /// The game crashed and wrote this crash report.
    Crashed { report: PathBuf },
    /// Prism could not start the game (Java, downloads, sign-in...); this log says why.
    Failed { log: Option<PathBuf> },
}

/// A Prism process started by [`launch`]. Dropping it leaves Prism and the game running.
pub struct Session {
    child: Child,
    _running: Running,
    game_dir: PathBuf,
    /// Prism's own log, for launches that fail before the game writes one.
    prism_log: Option<PathBuf>,
    started: SystemTime,
}

impl Session {
    fn start(mut cmd: Command, running: Running, instance: &Path, prism_data: Option<PathBuf>) -> Result<Self> {
        let child = cmd.spawn().context("starting Prism Launcher")?;
        Ok(Self {
            child,
            _running: running,
            game_dir: game_dir(instance),
            prism_log: prism_data.map(|d| d.join("logs").join("PrismLauncher-0.log")).filter(|p| p.is_file()),
            started: SystemTime::now(),
        })
    }

    /// Blocks until Prism exits (it quits with the game) and says what went wrong, if anything.
    /// When Prism was already open it only forwards the launch and exits at once, so nothing
    /// is seen then.
    pub fn wait(mut self) -> Option<Failure> {
        let ok = self.child.wait().is_ok_and(|s| s.success());
        if let Some(report) = newest(&self.game_dir.join("crash-reports"), self.started) {
            return Some(Failure::Crashed { report });
        }
        // Prism exits with 1 when the launch fails and its console stays closed.
        let game_log = self.game_dir.join("logs").join("latest.log");
        let fresh = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).is_ok_and(|m| m >= self.started);
        (!ok).then(|| Failure::Failed { log: Some(game_log).filter(|p| fresh(p)).or(self.prism_log.take()) })
    }
}

/// The newest file in `dir` modified at or after `since`.
fn newest(dir: &Path, since: SystemTime) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .filter(|(modified, _)| *modified >= since)
        .max()
        .map(|(_, path)| path)
}

/// Launches an instance, optionally joining a server (`host:port`) once the game is up, with
/// the account picked in BeaconSync (else Prism's default). The first launch without an
/// installed Prism downloads the built-in one (reported to `on`).
pub async fn launch(instance: &Path, s: &Settings, server: Option<&str>, on: OnProgress<'_>) -> Result<Session> {
    let id = instance.file_name().context("bad instance path")?;
    // Counted before checking for a sign-in, which checks the other way round, so one of the two
    // always sees the other.
    let running = Running::new();
    ensure!(!SIGNING_IN.load(Ordering::SeqCst), "Finish adding the Minecraft account first");
    let instances = if uses_builtin(s) { Some(s.instances_root()) } else { prism_instances_dir() };
    let same = |a: &Path, b: &Path| a == b || fs::canonicalize(a).ok().is_some_and(|a| fs::canonicalize(b).is_ok_and(|b| a == b));
    if !instances.as_deref().zip(instance.parent()).is_some_and(|(a, b)| same(a, b)) {
        bail!("This instance is outside Prism's instance folder. Move it there or set the instance folder in Settings.");
    }
    let (mut cmd, data) = match installed(s.prism_path.as_deref()) {
        Some(cmd) => (cmd, prism_data_dir()),
        None => (builtin::command(&builtin::ensure(on).await?, &builtin::data(), &s.instances_root())?, Some(builtin::data())),
    };
    cmd.arg("--launch").arg(id);
    if let Some(s) = server {
        cmd.arg("--server").arg(s);
    }
    // Prism refuses to launch with an unknown profile, so a removed account falls back to its default.
    if let Some(name) = &s.minecraft_account
        && accounts::list(s).iter().any(|a| &a.name == name)
    {
        cmd.arg("--profile").arg(name);
    }
    Session::start(cmd, running, instance, data)
}

/// Opens the installed Prism's window, where players manage its accounts.
pub fn open_installed(s: &Settings) -> Result<()> {
    installed(s.prism_path.as_deref()).context("Prism Launcher is not installed")?.spawn().context("starting Prism Launcher")?;
    Ok(())
}

/// Finds an installed Prism executable on this machine.
fn installed(override_path: Option<&Path>) -> Option<Command> {
    if let Some(p) = override_path.filter(|p| p.is_file()) {
        return Some(Command::new(p));
    }
    let mut candidates: Vec<PathBuf> = vec![];
    if cfg!(target_os = "macos") {
        candidates.push("/Applications/Prism Launcher.app/Contents/MacOS/prismlauncher".into());
        if let Some(h) = dirs::home_dir() {
            candidates.push(h.join("Applications/Prism Launcher.app/Contents/MacOS/prismlauncher"));
        }
    } else if cfg!(windows)
        && let Some(d) = dirs::data_local_dir()
    {
        candidates.push(d.join("Programs/PrismLauncher/prismlauncher.exe"));
    }
    if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
        return Some(Command::new(found));
    }
    let in_path = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|d| d.join(if cfg!(windows) { "prismlauncher.exe" } else { "prismlauncher" }))
        .find(|p| p.is_file());
    if let Some(p) = in_path {
        return Some(Command::new(p));
    }
    let flatpak = dirs::home_dir()?.join(".var/app/org.prismlauncher.PrismLauncher");
    flatpak.is_dir().then(|| {
        let mut c = Command::new("flatpak");
        c.args(["run", "org.prismlauncher.PrismLauncher"]);
        c
    })
}
