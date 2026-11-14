//! `beacon server`: install a pack's server, supervise it and apply deployments.
//!
//! `beacon server run` is the heart of service and docker modes: it runs Minecraft as a child
//! process, restarts it when it crashes, long polls the hub and applies new releases
//! (announce, stop, back up, update, restart, report). It also streams the console to the hub
//! and carries out owner actions (commands, start/stop, backups) sent from the app or CLI.

use crate::{
    backups, selfupdate,
    ui::{self, Bar},
};
use anyhow::{Context, Result, bail};
use beacon_core::{
    api::{AgentConsole, AgentReport, AgentTarget, NewServer, Release, Server, ServerAction, ServerMode},
    fetch::{Fetcher, Progress},
    hash::short,
    hub::Hub,
    import,
    model::{Commit, Tree},
    ops,
    server::{self as srv, ServerConfig},
    settings::Settings,
    store::ObjectStore,
};
use clap::Subcommand;
use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::OnceLock,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::mpsc,
};

#[derive(Subcommand)]
pub enum ServerCmd {
    /// Install one of your packs as a server on this machine (interactive).
    Install {
        /// Pack share code, id or name.
        #[arg(long)]
        pack: Option<String>,
        /// Folder to install into.
        #[arg(long)]
        path: Option<PathBuf>,
        #[command(flatten)]
        setup: Setup,
    },
    /// Bring a server you already run into BeaconSync (interactive): its mods and configs
    /// become a new pack, published as the first release, and the folder is linked in place.
    /// Worlds and server settings stay as they are.
    Import {
        /// Server folder, instead of the current one.
        path: Option<PathBuf>,
        /// Name of the new pack. Defaults to the server's MOTD.
        #[arg(long)]
        pack_name: Option<String>,
        /// Minecraft version, when it can't be detected.
        #[arg(long)]
        mc: Option<String>,
        /// Loader, when it can't be detected: fabric, quilt, forge or neoforge (optionally `fabric@0.16.9`).
        #[arg(long)]
        loader: Option<String>,
        /// Name of the first release.
        #[arg(long, default_value = FIRST_RELEASE)]
        release: String,
        /// Run it from a copy in this new folder, keeping the original untouched as a fallback.
        /// Asked when left out.
        #[arg(long)]
        into: Option<PathBuf>,
        #[command(flatten)]
        setup: Setup,
    },
    /// Run the server: supervise Minecraft and apply deployments automatically.
    Run { path: Option<PathBuf> },
    /// Apply the latest deployment once (for plain mode servers, stop Minecraft first).
    Update { path: Option<PathBuf> },
    /// Show what this server is running.
    Status { path: Option<PathBuf> },
    /// Go back to the newest backup while the server is stopped (e.g. after a bad update).
    Rollback { path: Option<PathBuf> },
    /// Watch the live console and type commands. Works from any machine you are signed in on.
    Console {
        #[command(flatten)]
        target: Target,
    },
    /// Send one console command, e.g. `beacon server cmd whitelist add Steve`.
    Cmd {
        #[command(flatten)]
        target: Target,
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,
    },
    /// List backups (yours and backup mods' like FTB Backups), or create, restore and delete them.
    Backups {
        #[command(flatten)]
        target: Target,
        #[command(subcommand)]
        action: Option<BackupCmd>,
    },
    /// Start Minecraft after `beacon server stop`.
    Start {
        #[command(flatten)]
        target: Target,
    },
    /// Stop Minecraft, keeping the server connected so you can start it again.
    Stop {
        #[command(flatten)]
        target: Target,
    },
    Restart {
        #[command(flatten)]
        target: Target,
    },
}

/// Which server to manage.
#[derive(clap::Args)]
pub struct Target {
    /// Server name or id, any of your servers. Defaults to the server in the current folder.
    #[arg(short, long)]
    pub server: Option<String>,
    /// Server folder, instead of the current one.
    #[arg(long)]
    pub path: Option<PathBuf>,
}

/// How a new server runs. Whatever is left out is asked for (all of it by default).
#[derive(clap::Args, Default)]
pub struct Setup {
    #[arg(long, value_enum)]
    mode: Option<ModeArg>,
    /// Server name shown in the app.
    #[arg(long)]
    name: Option<String>,
    /// Address players connect to (host:port).
    #[arg(long)]
    address: Option<String>,
    /// Memory for Minecraft in MB.
    #[arg(long)]
    memory: Option<u32>,
    /// Accept the Minecraft EULA (https://aka.ms/MinecraftEULA).
    #[arg(long)]
    accept_eula: bool,
    /// Do not start the server when done.
    #[arg(long)]
    no_start: bool,
}

#[derive(Subcommand)]
pub enum BackupCmd {
    /// Back up the whole server now (worlds included).
    Create,
    /// Restore a backup by id. Minecraft restarts; a safety backup is taken first.
    Restore {
        id: String,
    },
    Delete {
        id: String,
    },
}

#[derive(clap::ValueEnum, Clone, Copy)]
pub enum ModeArg {
    Service,
    Docker,
    Plain,
}

impl From<ModeArg> for ServerMode {
    fn from(m: ModeArg) -> Self {
        match m {
            ModeArg::Service => ServerMode::Service,
            ModeArg::Docker => ServerMode::Docker,
            ModeArg::Plain => ServerMode::Plain,
        }
    }
}

/// Release name an imported server's pack is published as, unless `--release` says otherwise.
pub const FIRST_RELEASE: &str = "v1.0";

pub async fn run(cmd: ServerCmd, global_dir: Option<PathBuf>) -> Result<()> {
    let pick = |p: Option<PathBuf>| -> Result<PathBuf> {
        let d = p.or(global_dir.clone()).unwrap_or(std::env::current_dir()?);
        Ok(d.canonicalize().unwrap_or(d))
    };
    match cmd {
        ServerCmd::Install { pack, path, setup } => install(pack, path, setup).await,
        ServerCmd::Import { path, pack_name, mc, loader, release, into, setup } => {
            import(pick(path)?, pack_name, mc, loader, &release, into, setup).await
        }
        ServerCmd::Run { path } => supervise(pick(path)?).await,
        ServerCmd::Update { path } => update_once(&pick(path)?).await,
        ServerCmd::Status { path } => status(&pick(path)?).await,
        ServerCmd::Rollback { path } => rollback(&pick(path)?).await,
        ServerCmd::Console { target } => {
            let (hub, server, version) = remote(target, global_dir.clone()).await?;
            crate::tui::run_server(hub, server, version).await
        }
        ServerCmd::Cmd { target, command } => {
            let command = command.join(" ");
            // On the server machine the console pipe works without the hub.
            if target.server.is_none() {
                let dir = pick(target.path.clone())?;
                let log = dir.join("logs/latest.log");
                let from = fs::metadata(&log).map_or(0, |m| m.len());
                if send_local(&dir, &command) {
                    ui::ok(format!("Sent `{command}`"));
                    tokio::time::sleep(REPLY_WAIT).await;
                    for l in log_since(&log, from).lines() {
                        println!("  {}", ui::dim(l));
                    }
                    return Ok(());
                }
            }
            act_remote(target, global_dir.clone(), ServerAction::Command(command), &[]).await
        }
        ServerCmd::Backups { target, action } => backups_cmd(target, global_dir.clone(), action).await,
        ServerCmd::Start { target } => {
            act_remote(target, global_dir.clone(), ServerAction::Start, &["starting:", "already running"]).await
        }
        ServerCmd::Stop { target } => {
            act_remote(target, global_dir.clone(), ServerAction::Stop, &["Minecraft stopped", "not running"]).await
        }
        ServerCmd::Restart { target } => act_remote(target, global_dir.clone(), ServerAction::Restart, &["starting:"]).await,
    }
}

fn log_progress(p: Progress) {
    if p.done == p.total || p.total <= 1 {
        say(format!("{} ({}/{})", p.label, p.done, p.total));
    }
}

fn hostname() -> String {
    fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "minecraft".into())
}

/// Half the machine's RAM, or what's free if that's less, clamped to 2-8 GB.
fn default_memory_mb() -> u32 {
    let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mb = |key: &str| -> Option<u64> {
        meminfo
            .lines()
            .find(|l| l.starts_with(key))
            .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
            .map(|kb| kb / 1024)
    };
    let half = mb("MemTotal:").unwrap_or(16 * 1024) / 2;
    // Machines that already run other servers: leave 1 GB of what's free for everything else.
    let fits = mb("MemAvailable:").map_or(half, |free| free.saturating_sub(1024));
    ((half.min(fits).clamp(2048, 8192) / 512) * 512) as u32
}

fn agent_hub(cfg: &ServerConfig) -> Option<Hub> {
    Some(Hub::new(cfg.hub.as_deref()?, Some(cfg.token.clone()?)))
}

async fn report(hub: &Option<Hub>, deployment: Option<String>, status: &str, message: impl Into<String>, commit: Option<String>) {
    if let Some(h) = hub
        && let Err(e) = h
            .agent_report(&AgentReport {
                deployment,
                status: status.into(),
                message: message.into(),
                commit,
                agent: Some(beacon_core::release::VERSION.into()),
            })
            .await
    {
        tracing::warn!("report failed: {e:#}");
    }
}

/// Downloads a commit's tree from the hub into the server's object cache.
async fn fetch_tree(hub: &Hub, store: &ObjectStore, commit: &str) -> Result<Tree> {
    if !store.has(commit) {
        store.put_verified(commit, &hub.get_object(commit).await?)?;
    }
    let c: Commit = store.get_json(commit)?;
    if !store.has(&c.tree) {
        store.put_verified(&c.tree, &hub.get_object(&c.tree).await?)?;
    }
    store.get_json(&c.tree)
}

/// Puts a `beacon` binary next to the server so services and containers run a stable path.
/// Containers always run Linux, so docker mode uses the static Linux build: `BEACON_LINUX_BINARY`
/// if set, else the latest release, else (on Linux) this executable.
async fn install_self(dir: &Path, mode: ServerMode) -> Result<PathBuf> {
    let name = if cfg!(windows) && mode != ServerMode::Docker { "beacon.exe" } else { "beacon" };
    let dest = dir.join(".beacon/bin").join(name);
    fs::create_dir_all(dest.parent().context("no parent")?)?;
    let me = std::env::current_exe()?;
    if mode == ServerMode::Docker {
        if let Some(local) = std::env::var_os("BEACON_LINUX_BINARY") {
            fs::copy(local, &dest)?;
        } else {
            let url = format!("{}/beacon-{}-unknown-linux-musl", beacon_core::companion::RELEASES, std::env::consts::ARCH);
            match beacon_core::http().get(&url).send().await.and_then(|r| r.error_for_status()) {
                Ok(resp) => fs::write(&dest, resp.bytes().await?)?,
                Err(_) if cfg!(target_os = "linux") => {
                    ui::warn("could not download the static Linux build, using this binary in the container");
                    fs::copy(&me, &dest)?;
                }
                Err(e) => bail!("could not download the Linux build of beacon for the container: {e}"),
            }
        }
    } else if me != dest {
        fs::copy(&me, &dest)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))?;
    }
    Ok(dest)
}

/// The [`Setup`] answers, after asking for what was not passed.
struct Answers {
    mode: ServerMode,
    name: String,
    address: String,
    memory_mb: u32,
}

fn ask(s: &Setup, default_name: String, port: u16) -> Result<Answers> {
    let mode = match s.mode {
        Some(m) => m.into(),
        None => {
            let items = [
                "Service: starts on boot, restarts on crash, updates on deploy (recommended)".to_string(),
                "Docker: same, inside a container".to_string(),
                "Plain: a folder with start.sh, you run it yourself".to_string(),
            ];
            [ServerMode::Service, ServerMode::Docker, ServerMode::Plain][ui::select("How should it run?", &items, 0)?]
        }
    };
    if mode == ServerMode::Service && !cfg!(target_os = "linux") {
        bail!("service mode uses systemd and needs Linux; pick docker or plain");
    }
    let name = match &s.name {
        Some(n) => n.clone(),
        None => ui::input("Server name", Some(default_name))?,
    };
    let address = match &s.address {
        Some(x) => x.clone(),
        None => ui::input("Address players connect to", Some(format!("{}:{port}", hostname())))?,
    };
    let memory_mb = match s.memory {
        Some(m) => m,
        None => ui::input("Memory for Minecraft (MB)", Some(default_memory_mb().to_string()))?.parse().context("not a number")?,
    };
    Ok(Answers { mode, name, address, memory_mb })
}

/// The first port from `from` nothing on this machine listens on, so a second server
/// doesn't default to one that's taken.
fn free_port(from: u16) -> u16 {
    (from..from.saturating_add(100)).find(|&p| std::net::TcpListener::bind(("0.0.0.0", p)).is_ok()).unwrap_or(from)
}

/// Asks to accept the EULA unless `--accept-eula` was passed or the server already did.
fn eula(s: &Setup, dir: &Path) -> Result<()> {
    let accepted = fs::read_to_string(dir.join("eula.txt")).is_ok_and(|t| t.lines().any(|l| l.trim() == "eula=true"));
    if !s.accept_eula && !accepted && !ui::confirm("Do you accept the Minecraft EULA (https://aka.ms/MinecraftEULA)?", true)? {
        bail!("the server can't start without accepting the EULA");
    }
    Ok(())
}

/// Registers the server with the hub and saves its config in `dir`.
async fn register(hub: &Hub, pack_id: &str, pack_code: &str, a: Answers, dir: &Path) -> Result<ServerConfig> {
    ui::info(format!("Registering {} with the hub", a.name));
    let created =
        hub.create_server(pack_id, &NewServer { name: a.name.clone(), mode: a.mode, address: a.address.clone() }).await?;
    let cfg = ServerConfig {
        format: 1,
        name: a.name,
        mode: a.mode,
        address: a.address,
        hub: Some(hub.base.clone()),
        pack_code: Some(pack_code.to_string()),
        server_id: Some(created.server.id.clone()),
        token: Some(created.token.clone()),
        memory_mb: a.memory_mb,
        jvm_args: vec!["-XX:+UseG1GC".into(), "-XX:+ParallelRefProcEnabled".into(), "-XX:MaxGCPauseMillis=200".into()],
        java: PathBuf::from("java"),
        start: vec![],
        commit: None,
        release_name: None,
        installed: None,
    };
    cfg.save(dir)?;
    Ok(cfg)
}

/// What happens to a new service or container once it is set up.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Launch {
    /// Start it now and on boot.
    Now,
    /// Start it on boot only (`--no-start`).
    OnBoot,
    /// Leave starting it to the user: another server still runs in the folder.
    Manual,
}

/// The last steps of install and import: EULA, `beacon` next to the server, loader and start
/// scripts, then the service or container.
async fn finish(dir: &Path, mut cfg: ServerConfig, tree: &Tree, release: &Release, launch: Launch, done: String) -> Result<()> {
    srv::accept_eula(dir)?;
    let beacon = install_self(dir, cfg.mode).await?;
    if cfg.mode != ServerMode::Docker {
        let bar = Bar::new();
        srv::ensure_loader(dir, &mut cfg, tree, &|p| bar.update(p)).await?;
        bar.done();
        srv::write_start_scripts(dir, &cfg)?;
    }
    cfg.commit = Some(release.commit.clone());
    cfg.release_name = Some(release.name.clone());
    cfg.save(dir)?;
    report(&agent_hub(&cfg), None, "idle", format!("installed {}", release.name), cfg.commit.clone()).await;
    ui::ok(done);
    match cfg.mode {
        ServerMode::Plain => {
            println!();
            let script = if cfg!(windows) { "start.bat" } else { "./start.sh" };
            println!("  Start it:      cd {} && {script}", dir.display());
            println!("  Auto updates:  {} server run {}", beacon.display(), dir.display());
            Ok(())
        }
        ServerMode::Service => setup_service(&cfg, &beacon, dir, launch),
        ServerMode::Docker => setup_docker(&cfg, tree, dir, launch),
    }
}

/// Installs a pack's server (`beacon server install`); also run by the TUI's "create server".
pub(crate) async fn install(pack: Option<String>, path: Option<PathBuf>, s: Setup) -> Result<()> {
    let settings = Settings::load();
    let hub = Hub::authed(&settings).context("run `beacon login` first")?;
    let packs: Vec<_> = hub.library().await?.into_iter().filter(|p| p.mine).collect();
    if packs.is_empty() {
        bail!("you have no published packs; publish one from the app first");
    }
    let pack = match &pack {
        Some(q) => packs
            .iter()
            .find(|p| p.code.eq_ignore_ascii_case(q) || &p.id == q || p.name.eq_ignore_ascii_case(q))
            .with_context(|| format!("no pack {q} in your library"))?,
        None => {
            let items: Vec<String> =
                packs.iter().map(|p| format!("{}  {} {} · {}", p.name, p.loader.kind.label(), p.minecraft, p.code)).collect();
            &packs[ui::select("Which pack?", &items, 0)?]
        }
    };
    let latest = pack.latest.clone().context("this pack has no release yet; publish one first")?;
    let a = ask(&s, format!("{} on {}", pack.name, hostname()), free_port(25565))?;
    let dir = match path {
        Some(p) => p,
        None => PathBuf::from(ui::input(
            "Install folder",
            Some(std::env::current_dir()?.join(srv::slug(&a.name)).to_string_lossy().into_owned()),
        )?),
    };
    if ServerConfig::path(&dir).exists() {
        bail!("{} already has a BeaconSync server; use `beacon server update`", dir.display());
    }
    eula(&s, &dir)?;
    fs::create_dir_all(&dir)?;
    let dir = dir.canonicalize()?;

    let cfg = register(&hub, &pack.id, &pack.code, a, &dir).await?;
    let store = ObjectStore::new(dir.join(".beacon/objects"));
    let tree = fetch_tree(&hub, &store, &latest.commit).await?;
    let bar = Bar::new();
    let fetcher = Fetcher::new(Some(store), Some(hub.base.clone()));
    srv::apply_tree(&dir, &tree, &fetcher, &|p| bar.update(p)).await?;
    bar.done();
    let launch = if s.no_start { Launch::OnBoot } else { Launch::Now };
    finish(&dir, cfg, &tree, &latest, launch, format!("Installed {} {} in {}", pack.name, latest.name, dir.display())).await
}

/// `server-port` from `server.properties`.
/// A non-empty `key=value` setting from the server's `server.properties`.
fn property(dir: &Path, key: &str) -> Option<String> {
    let props = fs::read_to_string(dir.join("server.properties")).ok()?;
    let v = props.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('='))?.trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// Brings an existing server in (`beacon server import`); also run by the TUI's "import server".
/// The server runs in place, or from a copy in `into` (asked when `None`) so the original
/// stays as it was.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn import(
    dir: PathBuf,
    pack_name: Option<String>,
    mc: Option<String>,
    loader: Option<String>,
    release: &str,
    into: Option<PathBuf>,
    s: Setup,
) -> Result<()> {
    // A config without a commit is an import that failed halfway; it is redone.
    if ServerConfig::load(&dir).is_ok_and(|c| c.commit.is_some()) {
        bail!("{} is already a BeaconSync server", dir.display());
    }
    if !dir.join("server.properties").is_file() && !dir.join("mods").is_dir() {
        bail!("{} doesn't look like a Minecraft server (no server.properties or mods folder)", dir.display());
    }
    let settings = Settings::load();
    let hub = Hub::authed(&settings).context("run `beacon login` first")?;
    // What detection can't tell (and --mc / --loader don't say) is asked for.
    let detected = import::detect_server(&dir);
    if let Err(e) = &detected
        && (mc.is_none() || loader.is_none())
    {
        ui::warn(format!("{e:#}"));
    }
    let minecraft = match (mc, &detected) {
        (Some(v), _) => v,
        (None, Ok((v, _))) => v.clone(),
        (None, Err(_)) => ui::input("Minecraft version", None)?,
    };
    let loader = match (loader, &detected) {
        (Some(l), _) => crate::pack::parse_loader(&l, &minecraft).await?,
        (None, Ok((_, l))) => l.clone(),
        (None, Err(_)) => {
            let l = ui::input("Loader: fabric, quilt, forge, neoforge or vanilla (optionally fabric@0.16.9)", None)?;
            crate::pack::parse_loader(&l, &minecraft).await?
        }
    };
    let mods = fs::read_dir(dir.join("mods"))
        .map(|r| r.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "jar")).count())
        .unwrap_or(0);
    ui::info(format!(
        "{} {} server for Minecraft {minecraft} with {mods} mods",
        loader.kind.label(),
        loader.version.as_deref().unwrap_or("")
    ));
    let pack_name = match pack_name {
        Some(n) => n,
        None => ui::input("Pack name", Some(import::server_name(&dir)))?,
    };
    let port = property(&dir, "server-port").and_then(|p| p.parse().ok()).unwrap_or(25565);
    let a = ask(&s, format!("{pack_name} on {}", hostname()), port)?;
    let into = match into {
        Some(t) => Some(t),
        None => {
            let copy = if writable(&dir) {
                let items =
                    [format!("Here, in {}", dir.display()), "From a copy in a new folder (the original stays as it is)".into()];
                ui::select("Where should it run?", &items, 0)? == 1
            } else {
                ui::warn(format!("{} can't write to {}, so it runs from a copy", current_user(), dir.display()));
                true
            };
            let default = std::env::current_dir()?.join(srv::slug(&a.name));
            copy.then(|| ui::input("New folder", Some(default.to_string_lossy().into_owned())).map(PathBuf::from)).transpose()?
        }
    };
    // A copy brings the compose file along, so the original decides in both cases.
    if a.mode == ServerMode::Docker && dir.join("docker-compose.yml").exists() {
        bail!("{} already has a docker-compose.yml; move it or pick another mode", dir.display());
    }
    eula(&s, &dir)?;
    // Minecraft may still be running from the original folder; then the switch is left to the user.
    let ip: std::net::IpAddr =
        property(&dir, "server-ip").and_then(|ip| ip.parse().ok()).unwrap_or(std::net::Ipv4Addr::LOCALHOST.into());
    let running = std::net::TcpStream::connect_timeout(&(ip, port).into(), Duration::from_millis(500)).is_ok();
    let dir = match into {
        Some(to) => {
            if running {
                ui::warn("The server is running, so its world may be mid-save in the copy. Stop it first for a clean copy.");
            }
            copy_server(&dir, &to)?
        }
        None => dir,
    };
    fs::create_dir_all(dir.join(".beacon")).with_context(|| format!("can't write to {}", dir.display()))?;

    ui::info("Saving the server's mods and configs as a pack");
    let repo =
        import::import_server(&dir, &settings.instances_root(), &pack_name, &minecraft, &loader, &crate::pack::author()).await?;
    let rel = ops::publish(&repo, &hub, release, "Imported from the server").await?;
    let remote = repo.config()?.remote.context("the pack was not published")?;
    ui::ok(format!("Published {pack_name} {} · share code {}", rel.name, remote.code));
    let tree = repo.commit_tree(&rel.commit)?;
    let cfg = register(&hub, &remote.pack, &remote.code, a, &dir).await?;
    // The pack came from these files, so nothing is downloaded over them.
    srv::mark_applied(&dir, &tree)?;
    let launch = match (running, s.no_start) {
        (true, _) => Launch::Manual,
        (false, true) => Launch::OnBoot,
        (false, false) => Launch::Now,
    };
    let done = format!("{} now follows {pack_name} {}", cfg.name, rel.name);
    finish(&dir, cfg, &tree, &rel, launch, done).await?;
    if running {
        ui::warn(format!("A server is still running on port {port}. Stop it, then start this one."));
    }
    println!("  Pack:     {}", repo.root.display());
    Ok(())
}

/// True when this user can create files in `dir`.
fn writable(dir: &Path) -> bool {
    let probe = dir.join(".beacon-write-test");
    let ok = fs::write(&probe, b"").is_ok();
    fs::remove_file(&probe).ok();
    ok
}

/// Copies a whole server (worlds included) into `to`, which must be new or empty, and returns
/// its absolute path. Symlinks are recreated, not followed. Files this user can't read (a server
/// run by Docker or another account) are copied with sudo, then handed to this user.
fn copy_server(from: &Path, to: &Path) -> Result<PathBuf> {
    let from = &from.canonicalize()?;
    if std::path::absolute(to)?.starts_with(from) {
        bail!("the copy can't go inside the server it copies");
    }
    if fs::read_dir(to).is_ok_and(|mut d| d.next().is_some()) {
        bail!("{} isn't empty; pick a new folder", to.display());
    }
    fs::create_dir_all(to).with_context(|| format!("can't create {}", to.display()))?;
    let to = to.canonicalize()?;
    ui::info(format!("Copying the server to {}", to.display()));
    let files = match copy_tree(from, &to) {
        Err(e) if !is_root() && permission_denied(&e) => {
            ui::warn(format!("{} can't read every file in {}; copying with sudo", current_user(), from.display()));
            sudo_copy(from, &to)?
        }
        r => r?,
    };
    ui::ok(format!("Copied {files} files; the original in {} is untouched", from.display()));
    Ok(to)
}

/// `copy_server`'s copy as this user, with a progress bar. Returns how many files it copied.
fn copy_tree(from: &Path, to: &Path) -> Result<usize> {
    let entries: Vec<_> = walkdir::WalkDir::new(from)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".beacon")
        .collect::<Result<_, _>>()
        .with_context(|| format!("can't read {}", from.display()))?;
    let files = entries.iter().filter(|e| e.file_type().is_file()).count();
    let bar = Bar::new();
    let mut done = 0;
    let copied = entries.iter().try_for_each(|e| -> Result<()> {
        let dest = to.join(e.path().strip_prefix(from)?);
        let t = e.file_type();
        if t.is_dir() {
            fs::create_dir_all(&dest)?;
        } else if t.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(fs::read_link(e.path())?, &dest)?;
        } else {
            fs::copy(e.path(), &dest).with_context(|| format!("copying {}", e.path().display()))?;
            done += 1;
            bar.update(Progress { done, total: files, label: e.file_name().to_string_lossy().into_owned() });
        }
        Ok(())
    });
    bar.done();
    copied.map(|()| files)
}

/// `copy_server`'s fallback: `cp -a` as root over whatever `copy_tree` got done, then the copy
/// is given to this user (owner, writable) and the original's `.beacon` dropped.
fn sudo_copy(from: &Path, to: &Path) -> Result<usize> {
    let (uid, gid) = uid_gid().context("copying with sudo needs a Unix system")?;
    let src = format!("{}/.", from.display());
    let dest = to.to_string_lossy();
    run_cmd("sudo", &["cp", "-a", &src, &dest], to)?;
    run_cmd("sudo", &["chown", "-R", &format!("{uid}:{gid}"), &dest], to)?;
    // `cp -a` keeps the original's modes; the server still has to write to its copy.
    run_cmd("sudo", &["chmod", "-R", "u+rwX", &dest], to)?;
    let beacon = to.join(".beacon");
    if beacon.exists() {
        fs::remove_dir_all(&beacon)?;
    }
    Ok(walkdir::WalkDir::new(to).into_iter().flatten().filter(|e| e.file_type().is_file()).count())
}

/// True when `e` comes from the OS refusing access.
fn permission_denied(e: &anyhow::Error) -> bool {
    e.chain().any(|c| c.downcast_ref::<std::io::Error>().is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied))
}

fn run_cmd(program: &str, args: &[&str], dir: &Path) -> Result<()> {
    let status =
        std::process::Command::new(program).args(args).current_dir(dir).status().with_context(|| format!("running {program}"))?;
    if !status.success() {
        bail!("{program} {} failed", args.join(" "));
    }
    Ok(())
}

fn is_root() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    false
}

fn current_user() -> String {
    std::env::var("SUDO_USER").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "root".into())
}

fn setup_service(cfg: &ServerConfig, beacon: &Path, dir: &Path, launch: Launch) -> Result<()> {
    let name = srv::service_name(cfg);
    let unit_file = dir.join(format!("{name}.service"));
    fs::write(&unit_file, srv::systemd_unit(cfg, beacon, dir, &current_user()))?;
    let target = format!("/etc/systemd/system/{name}.service");
    let unit = unit_file.to_string_lossy().into_owned();
    let sudo =
        |args: &[&str]| -> Result<()> { if is_root() { run_cmd(args[0], &args[1..], dir) } else { run_cmd("sudo", args, dir) } };
    if !is_root() {
        ui::info("Installing the systemd service needs sudo");
    }
    sudo(&["install", "-m", "644", &unit, &target])?;
    sudo(&["systemctl", "daemon-reload"])?;
    let state = match launch {
        Launch::Now => sudo(&["systemctl", "enable", "--now", &name]).map(|_| "started"),
        Launch::OnBoot => sudo(&["systemctl", "enable", &name]).map(|_| "enabled"),
        Launch::Manual => Ok("installed"),
    }?;
    ui::ok(format!("Service {name} {state}"));
    if launch == Launch::Manual {
        println!("  Start:    sudo systemctl enable --now {name}");
    }
    println!("  Logs:     journalctl -u {name} -f");
    println!("  Restart:  sudo systemctl restart {name}");
    Ok(())
}

#[cfg(unix)]
fn uid_gid() -> Option<(u32, u32)> {
    unsafe { Some((libc::getuid(), libc::getgid())) }
}

#[cfg(not(unix))]
fn uid_gid() -> Option<(u32, u32)> {
    None
}

fn setup_docker(cfg: &ServerConfig, tree: &Tree, dir: &Path, launch: Launch) -> Result<()> {
    fs::create_dir_all(dir.join(".beacon/home"))?;
    let java = beacon_core::meta::java_for(&tree.minecraft);
    fs::write(dir.join("docker-compose.yml"), srv::docker_compose(cfg, java, uid_gid()))?;
    ui::ok("Wrote docker-compose.yml");
    let compose = dir.join("docker-compose.yml");
    if launch == Launch::Now {
        run_cmd("docker", &["compose", "up", "-d"], dir)?;
        ui::ok("Container started (first start installs the loader, give it a minute)");
    } else {
        println!("  Start:    docker compose -f {} up -d", compose.display());
    }
    println!("  Logs:     docker compose -f {} logs -f", compose.display());
    println!("  Console:  docker attach beacon-{}", srv::slug(&cfg.name));
    Ok(())
}

// ---- supervisor ---------------------------------------------------------------------------

/// Console output of the supervised server, uploaded to the hub so owners can watch it.
static CONSOLE: OnceLock<mpsc::UnboundedSender<String>> = OnceLock::new();

/// Prints a console line (journal and docker logs keep working) and uploads it.
fn out(line: String) {
    println!("{line}");
    if let Some(tx) = CONSOLE.get() {
        tx.send(line).ok();
    }
}

/// A supervisor message, shown in the console with a `[beacon]` prefix.
fn say(msg: impl std::fmt::Display) {
    out(format!("[beacon] {msg}"));
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip a CSI sequence: ESC [ params final-byte.
            if chars.next() == Some('[') {
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Sends console lines to the hub in small batches.
async fn uploader(hub: Hub, mut rx: mpsc::UnboundedReceiver<String>) {
    while let Some(first) = rx.recv().await {
        let mut lines = vec![strip_ansi(&first)];
        let until = tokio::time::Instant::now() + Duration::from_millis(250);
        while lines.len() < 500 {
            match tokio::time::timeout_at(until, rx.recv()).await {
                Ok(Some(l)) => lines.push(strip_ansi(&l)),
                _ => break,
            }
        }
        if let Err(e) = hub.agent_console(&AgentConsole { lines, ..Default::default() }).await {
            tracing::warn!("console upload failed: {e:#}");
        }
    }
}

/// Tells the hub whether Minecraft runs and which backups exist.
async fn push_state(hub: &Option<Hub>, dir: &Path, running: bool) {
    let Some(h) = hub else { return };
    let d = dir.to_path_buf();
    let backups = tokio::task::spawn_blocking(move || backups::list(&d)).await.unwrap_or_default();
    let c = AgentConsole { lines: vec![], backups: Some(backups), running: Some(running) };
    if let Err(e) = h.agent_console(&c).await {
        tracing::warn!("state upload failed: {e:#}");
    }
}

/// Pid file preventing two supervisors (or a supervisor and `update`) on one server.
struct RunLock(PathBuf);

fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn running_pid(dir: &Path) -> Option<u32> {
    let pid: u32 = fs::read_to_string(dir.join(".beacon/run.pid")).ok()?.trim().parse().ok()?;
    (pid != std::process::id() && pid_alive(pid)).then_some(pid)
}

impl RunLock {
    fn acquire(dir: &Path) -> Result<Self> {
        if let Some(pid) = running_pid(dir) {
            bail!("this server is already running (pid {pid})");
        }
        let p = dir.join(".beacon/run.pid");
        fs::write(&p, std::process::id().to_string())?;
        Ok(Self(p))
    }
}

impl Drop for RunLock {
    fn drop(&mut self) {
        fs::remove_file(&self.0).ok();
    }
}

struct Mc {
    child: Child,
    /// Held separately: `Child::wait` would otherwise close Minecraft's console input.
    stdin: Option<tokio::process::ChildStdin>,
}

/// Forwards every line Minecraft prints to [`out`].
fn pump(r: impl tokio::io::AsyncRead + Unpin + Send + 'static) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(r).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            out(line);
        }
    });
}

impl Mc {
    fn start(dir: &Path, cfg: &ServerConfig) -> Result<Self> {
        srv::set_server_port(dir, cfg)?;
        let argv = cfg.argv();
        say(format!("starting: {}", argv.join(" ")));
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..]).current_dir(dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        // Own process group so Ctrl+C reaches us, and we stop Minecraft cleanly.
        #[cfg(unix)]
        cmd.process_group(0);
        let mut child = cmd.spawn().context("starting Minecraft")?;
        if let Some(o) = child.stdout.take() {
            pump(o);
        }
        if let Some(e) = child.stderr.take() {
            pump(e);
        }
        let stdin = child.stdin.take();
        Ok(Self { child, stdin })
    }

    async fn send(&mut self, line: &str) {
        if let Some(stdin) = self.stdin.as_mut() {
            stdin.write_all(format!("{line}\n").as_bytes()).await.ok();
            stdin.flush().await.ok();
        }
    }

    /// Sends `stop` and waits, killing Minecraft if it doesn't exit in time.
    async fn stop(mut self) {
        self.send("stop").await;
        if tokio::time::timeout(Duration::from_secs(90), self.child.wait()).await.is_err() {
            say("Minecraft did not stop in 90s, killing it");
            self.child.kill().await.ok();
        }
    }
}

async fn wait_exit(mc: &mut Option<Mc>) -> std::io::Result<std::process::ExitStatus> {
    match mc {
        Some(m) => m.child.wait().await,
        None => std::future::pending().await,
    }
}

/// Long polls the hub for deployments and owner actions.
async fn agent_loop(hub: Hub, targets: mpsc::Sender<AgentTarget>, actions: mpsc::Sender<ServerAction>) {
    let mut last: Option<String> = None;
    loop {
        match hub.agent_poll(25, last.as_deref()).await {
            Ok(p) => {
                for a in p.actions {
                    if actions.send(a).await.is_err() {
                        return;
                    }
                }
                if let Some(t) = p.target
                    && last.as_ref() != Some(&t.deployment)
                {
                    last = Some(t.deployment.clone());
                    if targets.send(t).await.is_err() {
                        return;
                    }
                }
            }
            Err(e) => {
                tracing::warn!("hub poll failed: {e:#}");
                tokio::time::sleep(Duration::from_secs(15)).await;
            }
        }
    }
}

fn console_path(dir: &Path) -> PathBuf {
    dir.join(".beacon/console")
}

/// Named pipe at `.beacon/console`: lines written to it go to the Minecraft console, so
/// `beacon server cmd` works for services and containers that have no terminal.
#[cfg(unix)]
fn console_pipe(dir: &Path, tx: mpsc::Sender<String>) -> Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let path = console_path(dir);
    fs::remove_file(&path).ok();
    let c = CString::new(path.as_os_str().as_bytes())?;
    if unsafe { libc::mkfifo(c.as_ptr(), 0o600) } != 0 {
        bail!("cannot create console pipe: {}", std::io::Error::last_os_error());
    }
    std::thread::spawn(move || {
        loop {
            // Blocks until a writer connects; reopen after each writer closes.
            let Ok(f) = fs::File::open(&path) else { return };
            for line in std::io::BufRead::lines(std::io::BufReader::new(f)).map_while(Result::ok) {
                if tx.blocking_send(line).is_err() {
                    return;
                }
            }
        }
    });
    Ok(())
}

/// Writes a command to the local console pipe. False when no supervisor listens.
/// How long `beacon server cmd` shows the server's output for after sending a command.
const REPLY_WAIT: Duration = Duration::from_millis(1500);

/// What was written to `log` after byte `from` (empty if it was rotated meanwhile).
fn log_since(log: &Path, from: u64) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let mut out = String::new();
    if let Ok(mut f) = fs::File::open(log)
        && f.seek(SeekFrom::Start(from)).is_ok()
    {
        f.read_to_string(&mut out).ok();
    }
    out
}

fn send_local(dir: &Path, command: &str) -> bool {
    #[cfg(unix)]
    {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(console_path(dir))
            .and_then(|mut pipe| writeln!(pipe, "{command}"))
            .is_ok()
    }
    #[cfg(not(unix))]
    {
        let _ = (dir, command);
        false
    }
}

async fn stdin_loop(tx: mpsc::Sender<String>) {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if tx.send(line).await.is_err() {
            return;
        }
    }
}

async fn supervise(dir: PathBuf) -> Result<()> {
    let _lock = RunLock::acquire(&dir)?;
    let mut cfg = ServerConfig::load(&dir)?;
    let store = ObjectStore::new(dir.join(".beacon/objects"));
    let agent = agent_hub(&cfg);
    if let Some(h) = agent.clone() {
        let (tx, rx) = mpsc::unbounded_channel();
        CONSOLE.set(tx).ok();
        tokio::spawn(uploader(h, rx));
    }
    if let Some(tree) = srv::applied_tree(&dir) {
        srv::ensure_loader(&dir, &mut cfg, &tree, &log_progress).await?;
        if cfg.mode != ServerMode::Docker {
            srv::write_start_scripts(&dir, &cfg)?;
        }
        cfg.save(&dir)?;
    }
    if cfg.start.is_empty() {
        bail!("no start command; reinstall with `beacon server install`");
    }

    let (tx_target, mut targets) = mpsc::channel(1);
    let (tx_action, mut actions) = mpsc::channel(16);
    if let Some(h) = agent.clone() {
        tokio::spawn(agent_loop(h, tx_target, tx_action));
    }
    let (tx_in, mut console) = mpsc::channel(16);
    #[cfg(unix)]
    console_pipe(&dir, tx_in.clone())?;
    tokio::spawn(stdin_loop(tx_in));
    #[cfg(unix)]
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    let mut mc = Some(Mc::start(&dir, &cfg)?);
    report(&agent, None, "running", format!("running {}", cfg.release_name.clone().unwrap_or_default()), cfg.commit.clone())
        .await;
    push_state(&agent, &dir, true).await;
    let mut crashes: VecDeque<Instant> = VecDeque::new();

    loop {
        #[cfg(unix)]
        let terminate = term.recv();
        #[cfg(not(unix))]
        let terminate = std::future::pending::<Option<()>>();
        tokio::select! {
            status = wait_exit(&mut mc) => {
                let code = status.map(|s| s.to_string()).unwrap_or_else(|e| e.to_string());
                crashes.push_back(Instant::now());
                while crashes.front().is_some_and(|t| t.elapsed() > Duration::from_secs(600)) {
                    crashes.pop_front();
                }
                let wait = if crashes.len() >= 5 { 300 } else { 10 };
                say(format!("Minecraft exited ({code}); restarting in {wait}s"));
                report(&agent, None, "failed", format!("Minecraft exited ({code}), restarting"), cfg.commit.clone()).await;
                push_state(&agent, &dir, false).await;
                tokio::time::sleep(Duration::from_secs(wait)).await;
                mc = Some(Mc::start(&dir, &cfg)?);
                report(&agent, None, "running", "restarted after a crash", cfg.commit.clone()).await;
                push_state(&agent, &dir, true).await;
            }
            Some(target) = targets.recv() => {
                let hub = agent.clone().context("no hub")?;
                apply_update(&dir, &mut cfg, &store, &hub, target, mc.take()).await;
                mc = Some(Mc::start(&dir, &cfg)?);
                push_state(&agent, &dir, true).await;
            }
            Some(a) = actions.recv() => {
                if let ServerAction::Update(version) = a {
                    // Only returns when the update didn't happen. Minecraft comes back if it was running.
                    let was_running = mc.is_some();
                    let e = self_update(&agent, &mut mc, &version).await;
                    say(format!("beacon update failed: {e:#}"));
                    if was_running && mc.is_none() {
                        mc = Some(Mc::start(&dir, &cfg)?);
                    }
                    report(&agent, None, "running", format!("beacon update failed: {e:#}"), cfg.commit.clone()).await;
                } else if let Err(e) = act(&dir, &mut cfg, &agent, &mut mc, a).await {
                    say(format!("failed: {e:#}"));
                }
                push_state(&agent, &dir, mc.is_some()).await;
            }
            Some(line) = console.recv() => {
                if let Some(m) = mc.as_mut() {
                    m.send(&line).await;
                }
            }
            _ = tokio::signal::ctrl_c() => break,
            _ = terminate => break,
        }
    }
    say("stopping Minecraft");
    if let Some(m) = mc {
        m.stop().await;
    }
    report(&agent, None, "stopped", "server stopped", cfg.commit.clone()).await;
    push_state(&agent, &dir, false).await;
    Ok(())
}

pub(crate) fn size(bytes: u64) -> String {
    match bytes {
        ..1_048_576 => format!("{} KB", bytes.div_ceil(1024)),
        1_048_576..1_073_741_824 => format!("{:.0} MB", bytes as f64 / 1_048_576.0),
        _ => format!("{:.1} GB", bytes as f64 / 1_073_741_824.0),
    }
}

/// Carries out an owner's action from the app or CLI.
async fn act(dir: &Path, cfg: &mut ServerConfig, agent: &Option<Hub>, mc: &mut Option<Mc>, a: ServerAction) -> Result<()> {
    let release = cfg.release_name.clone().unwrap_or_default();
    match a {
        ServerAction::Command(c) => {
            let m = mc.as_mut().context("Minecraft is stopped; start it first")?;
            out(format!("> {c}"));
            m.send(&c).await;
        }
        ServerAction::Start if mc.is_some() => say("Minecraft is already running"),
        ServerAction::Start => {
            *mc = Some(Mc::start(dir, cfg)?);
            report(agent, None, "running", format!("running {release}"), cfg.commit.clone()).await;
        }
        ServerAction::Stop => match mc.take() {
            Some(m) => {
                say("stopping Minecraft");
                m.stop().await;
                say("Minecraft stopped. Start it again from the app or with `beacon server start`.");
                report(agent, None, "stopped", "stopped by the owner", cfg.commit.clone()).await;
            }
            None => say("Minecraft is not running"),
        },
        ServerAction::Restart => {
            say("restarting Minecraft");
            if let Some(m) = mc.take() {
                m.stop().await;
            }
            *mc = Some(Mc::start(dir, cfg)?);
            report(agent, None, "running", format!("running {release}"), cfg.commit.clone()).await;
        }
        ServerAction::Backup => {
            say("backing up the server");
            // Flush the world and pause saving so the archive is consistent.
            if let Some(m) = mc.as_mut() {
                m.send("save-off").await;
                m.send("save-all flush").await;
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            let d = dir.to_path_buf();
            let made = tokio::task::spawn_blocking(move || backups::create(&d, "manual").inspect(|_| backups::prune(&d))).await?;
            if let Some(m) = mc.as_mut() {
                m.send("save-on").await;
            }
            let b = made?;
            say(format!("backup done: {} ({})", b.id, size(b.size)));
        }
        ServerAction::Restore(id) => {
            say(format!("restoring {id}"));
            if let Some(mut m) = mc.take() {
                m.send("say [BeaconSync] Restoring a backup, back in a minute").await;
                tokio::time::sleep(Duration::from_secs(3)).await;
                m.stop().await;
            }
            let before = cfg.commit.clone();
            let d = dir.to_path_buf();
            let restored = tokio::task::spawn_blocking(move || -> Result<()> {
                say("taking a safety backup first");
                let safety = backups::create(&d, "before restore")?;
                if let Err(e) = backups::restore(&d, &id) {
                    backups::restore(&d, &safety.id)?;
                    return Err(e);
                }
                backups::prune(&d);
                Ok(())
            })
            .await?;
            *cfg = ServerConfig::load(dir)?;
            *mc = Some(Mc::start(dir, cfg)?);
            restored?;
            let now = cfg.release_name.clone().unwrap_or_default();
            say(format!("restored, running {now}"));
            // A whole server restore can bring back an older pack version; the hub must not
            // push the newer deployment again until the next deploy.
            let status = if cfg.commit != before { "rolledback" } else { "running" };
            report(agent, None, status, format!("restored a backup, running {now}"), cfg.commit.clone()).await;
        }
        ServerAction::DeleteBackup(id) => {
            backups::delete(dir, &id)?;
            say(format!("deleted backup {id}"));
        }
        ServerAction::Update(_) => unreachable!("handled by supervise"),
    }
    Ok(())
}

/// Updates this `beacon` to `version` and restarts into it. The new build is downloaded and
/// checked (GitHub's sha256, then `--version`) while Minecraft keeps running; only then are
/// players warned and Minecraft stopped. Returns only on failure.
async fn self_update(agent: &Option<Hub>, mc: &mut Option<Mc>, version: &str) -> anyhow::Error {
    let staged = async {
        use beacon_core::release;
        if !release::newer(version, release::VERSION) {
            bail!("already on beacon {} (asked for {version})", release::VERSION);
        }
        say(format!("updating beacon {} to {version}", release::VERSION));
        report(agent, None, "applying", format!("updating beacon to {version}"), None).await;
        let rel = release::by_version(version).await?;
        say(format!("downloading beacon {version}"));
        let staged = selfupdate::stage(&rel, |_, _| {}).await?;
        say("verified");
        Ok(staged)
    }
    .await;
    let staged = match staged {
        Ok(staged) => staged,
        Err(e) => return e,
    };
    if let Some(mut m) = mc.take() {
        m.send("say [BeaconSync] Restarting for a BeaconSync update in 30 seconds").await;
        tokio::time::sleep(Duration::from_secs(20)).await;
        m.send("say [BeaconSync] Restarting in 10 seconds").await;
        tokio::time::sleep(Duration::from_secs(10)).await;
        m.stop().await;
    }
    say(format!("restarting on beacon {version}"));
    match restart_into(staged) {
        Ok(never) => match never {},
        Err(e) => e,
    }
}

/// Swaps the running executable for the staged build and becomes it, with the same arguments.
/// On Unix this is `exec`: same pid, so systemd and docker don't notice. On Windows a new
/// process takes over.
fn restart_into(staged: selfupdate::Staged) -> Result<std::convert::Infallible> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let exe = staged.exe.clone();
    let old = staged.swap()?;
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = std::process::Command::new(&exe).args(&args).exec();
        // Put the old build back so the next start still works.
        fs::rename(&old, &exe).ok();
        Err(e.into())
    }
    #[cfg(not(unix))]
    {
        let _ = old;
        std::process::Command::new(&exe).args(&args).spawn()?;
        std::process::exit(0)
    }
}

/// Announces, stops Minecraft, backs up, applies the new version and reports. On failure
/// the backup is restored so the server comes back on the previous version.
async fn apply_update(dir: &Path, cfg: &mut ServerConfig, store: &ObjectStore, hub: &Hub, t: AgentTarget, mc: Option<Mc>) {
    let agent = Some(hub.clone());
    let dep = Some(t.deployment.clone());
    say(format!("deploying {} ({})", t.release_name, short(&t.commit)));
    report(&agent, dep.clone(), "applying", format!("updating to {}", t.release_name), cfg.commit.clone()).await;
    let result: Result<()> = async {
        let tree = fetch_tree(hub, store, &t.commit).await?;
        if let Some(mut m) = mc {
            m.send(&format!("say [BeaconSync] Updating to {} in 30 seconds", t.release_name)).await;
            tokio::time::sleep(Duration::from_secs(20)).await;
            m.send("say [BeaconSync] Restarting in 10 seconds").await;
            tokio::time::sleep(Duration::from_secs(10)).await;
            m.stop().await;
        }
        apply_with_backup(dir, cfg, store, hub, &tree, &t).await
    }
    .await;
    match result {
        Ok(()) => {
            say(format!("now on {}", t.release_name));
            report(&agent, dep, "done", format!("running {}", t.release_name), cfg.commit.clone()).await;
        }
        Err(e) => {
            say(format!("update failed: {e:#}"));
            report(&agent, dep, "failed", format!("update failed: {e:#}"), cfg.commit.clone()).await;
        }
    }
}

async fn apply_with_backup(
    dir: &Path,
    cfg: &mut ServerConfig,
    store: &ObjectStore,
    hub: &Hub,
    tree: &Tree,
    t: &AgentTarget,
) -> Result<()> {
    say("backing up");
    let (d, note) = (dir.to_path_buf(), format!("before {}", t.release_name));
    let backup = tokio::task::spawn_blocking(move || backups::create(&d, &note)).await??;
    let fetcher = Fetcher::new(Some(store.clone()), Some(hub.base.clone()));
    let applied: Result<()> = async {
        srv::apply_tree(dir, tree, &fetcher, &log_progress).await?;
        srv::ensure_loader(dir, cfg, tree, &log_progress).await?;
        if cfg.mode != ServerMode::Docker {
            srv::write_start_scripts(dir, cfg)?;
        }
        Ok(())
    }
    .await;
    if let Err(e) = applied {
        say("restoring the backup");
        let d = dir.to_path_buf();
        tokio::task::spawn_blocking(move || backups::restore(&d, &backup.id)).await??;
        *cfg = ServerConfig::load(dir)?;
        return Err(e);
    }
    backups::prune(dir);
    cfg.commit = Some(t.commit.clone());
    cfg.release_name = Some(t.release_name.clone());
    cfg.save(dir)
}

async fn update_once(dir: &Path) -> Result<()> {
    if running_pid(dir).is_some() {
        bail!("this server runs under `beacon server run`, which applies updates automatically");
    }
    let mut cfg = ServerConfig::load(dir)?;
    let hub = agent_hub(&cfg).context("server is not linked to a hub")?;
    let poll = hub.agent_poll(0, None).await?;
    let Some(t) = poll.target else {
        ui::ok(format!("Up to date ({})", cfg.release_name.clone().unwrap_or_default()));
        return Ok(());
    };
    if !ui::confirm(&format!("Update to {}? Stop Minecraft first.", t.release_name), true)? {
        return Ok(());
    }
    let store = ObjectStore::new(dir.join(".beacon/objects"));
    let agent = Some(hub.clone());
    report(&agent, Some(t.deployment.clone()), "applying", format!("updating to {}", t.release_name), cfg.commit.clone()).await;
    let tree = fetch_tree(&hub, &store, &t.commit).await?;
    match apply_with_backup(dir, &mut cfg, &store, &hub, &tree, &t).await {
        Ok(()) => {
            report(&agent, Some(t.deployment), "done", format!("updated to {}", t.release_name), cfg.commit.clone()).await;
            ui::ok(format!("Updated to {}. Start the server again.", t.release_name));
            Ok(())
        }
        Err(e) => {
            report(&agent, Some(t.deployment), "failed", format!("{e:#}"), cfg.commit.clone()).await;
            Err(e)
        }
    }
}

async fn status(dir: &Path) -> Result<()> {
    let cfg = ServerConfig::load(dir)?;
    println!("{}  {}", ui::bold(&cfg.name), ui::dim(dir.display()));
    println!("  Mode      {:?}", cfg.mode);
    println!("  Address   {}", cfg.address);
    println!(
        "  Version   {} {}",
        cfg.release_name.as_deref().unwrap_or("?"),
        ui::dim(cfg.commit.as_deref().map(short).unwrap_or(""))
    );
    if let Some((mc, l)) = &cfg.installed {
        println!("  Minecraft {mc} {} {}", l.kind.label(), l.version.as_deref().unwrap_or(""));
    }
    let running = if cfg.mode == ServerMode::Docker {
        // The supervisor lives inside the container, so ask docker.
        let name = format!("beacon-{}", srv::slug(&cfg.name));
        std::process::Command::new("docker")
            .args(["inspect", "-f", "{{.State.Status}}", &name])
            .output()
            .ok()
            .map(|o| format!("{} (container {name})", String::from_utf8_lossy(&o.stdout).trim()))
            .unwrap_or_else(|| "unknown (docker not available)".into())
    } else if running_pid(dir).is_some() {
        "yes".into()
    } else {
        "no (or not supervised)".into()
    };
    println!("  Running   {running}");
    let all = backups::list(dir);
    match all.first() {
        Some(b) => println!("  Backups   {} · newest {} ({})", all.len(), ui::ago(b.created), b.source),
        None => println!("  Backups   none yet"),
    }
    if let Some(hub) = agent_hub(&cfg) {
        match hub.agent_poll(0, None).await {
            Ok(p) => match p.target {
                Some(t) => println!("  Pending   {} (deploying)", t.release_name),
                None => println!("  Hub       {} · up to date", hub.base),
            },
            Err(e) => println!("  Hub       unreachable: {e:#}"),
        }
    }
    Ok(())
}

async fn rollback(dir: &Path) -> Result<()> {
    if running_pid(dir).is_some() {
        bail!("the server is running; use `beacon server backups restore <id>` to restore while it runs");
    }
    let b = backups::latest(dir).context("no backup to restore")?;
    if !ui::confirm(
        &format!("Restore the backup from {} ({})? World changes since then are lost.", ui::ago(b.created), b.note),
        false,
    )? {
        return Ok(());
    }
    let d = dir.to_path_buf();
    tokio::task::spawn_blocking(move || backups::restore(&d, &b.id)).await??;
    let cfg = ServerConfig::load(dir)?;
    // The hub won't push the rolled back deployment again until a new deploy.
    report(
        &agent_hub(&cfg),
        None,
        "rolledback",
        format!("rolled back to {}", cfg.release_name.clone().unwrap_or_default()),
        cfg.commit.clone(),
    )
    .await;
    ui::ok(format!("Rolled back to {}. Start the server again.", cfg.release_name.unwrap_or_default()));
    Ok(())
}

// ---- managing a server from anywhere --------------------------------------------------------

/// The hub and server to manage, and the release it runs: `--server` among your packs'
/// servers, else the server installed in the folder.
pub async fn remote(t: Target, global_dir: Option<PathBuf>) -> Result<(Hub, Server, String)> {
    let s = Settings::load();
    let token = s.token.clone().context("run `beacon login` first")?;
    if let Some(q) = t.server {
        let hub = Hub::authed(&s)?;
        let mut found = vec![];
        for p in hub.library().await?.into_iter().filter(|p| p.mine) {
            let matching = hub.servers(&p.id).await?.into_iter().filter(|x| x.id == q || x.name.eq_ignore_ascii_case(&q));
            found.extend(matching.map(|x| (p.id.clone(), x)));
        }
        let (pack, server) = match found.len() {
            0 => bail!("no server named {q} in your packs"),
            1 => found.remove(0),
            _ => bail!("{q} matches several servers; pass its id"),
        };
        let commit = server.current_commit.clone().unwrap_or_default();
        let version = hub
            .releases(&pack)
            .await?
            .into_iter()
            .find(|r| r.commit == commit)
            .map_or_else(|| short(&commit).to_string(), |r| r.name);
        return Ok((hub, server, version));
    }
    let dir = t.path.or(global_dir).unwrap_or(std::env::current_dir()?);
    let cfg = ServerConfig::load(&dir).context("not a server folder; pass --server <name>")?;
    let (Some(url), Some(id)) = (cfg.hub.clone(), cfg.server_id.clone()) else { bail!("this server is not linked to a hub") };
    let version = cfg.release_name.clone().unwrap_or_default();
    let server = Server {
        id,
        name: cfg.name,
        mode: cfg.mode,
        address: cfg.address,
        current_commit: cfg.commit,
        target_commit: None,
        target_release: None,
        status: String::new(),
        message: String::new(),
        last_seen: None,
        online: true,
        agent_version: Some(beacon_core::release::VERSION.into()),
    };
    Ok((Hub::new(&url, Some(token)), server, version))
}

/// Sends an action and prints the server's `[beacon]` replies until one says it finished.
async fn act_remote(t: Target, global_dir: Option<PathBuf>, a: ServerAction, done: &[&str]) -> Result<()> {
    let (hub, server, _) = remote(t, global_dir).await?;
    let mut next = hub.console(&server.id, None, 0).await?.next;
    hub.server_action(&server.id, &a).await?;
    ui::info(format!("Sent to {}", server.name));
    // A console command: show what the server answers. It comes back through the hub within
    // seconds; once it does, a little longer catches the rest of a multi-line reply.
    if done.is_empty() {
        let mut until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until {
            let v = hub.console(&server.id, Some(next), 1).await?;
            next = v.next;
            for l in v.lines.iter().filter(|l| !l.starts_with('>')) {
                println!("  {}", ui::dim(l));
                until = until.min(Instant::now() + REPLY_WAIT);
            }
        }
        return Ok(());
    }
    let deadline = Instant::now() + Duration::from_secs(600);
    while Instant::now() < deadline {
        let v = hub.console(&server.id, Some(next), 20).await?;
        next = v.next;
        for l in v.lines.iter().filter(|l| l.starts_with("[beacon]") || l.starts_with('>')) {
            println!("  {}", ui::dim(l));
            if l.contains("failed:") {
                bail!("{}", l.trim_start_matches("[beacon] failed: "));
            }
            if done.iter().any(|d| l.contains(d)) {
                return Ok(());
            }
        }
    }
    Ok(())
}

async fn backups_cmd(t: Target, global_dir: Option<PathBuf>, action: Option<BackupCmd>) -> Result<()> {
    match action {
        Some(BackupCmd::Create) => act_remote(t, global_dir, ServerAction::Backup, &["backup done"]).await,
        Some(BackupCmd::Restore { id }) => {
            if !ui::confirm(&format!("Restore {id}? Minecraft restarts and world changes since then are lost."), false)? {
                return Ok(());
            }
            act_remote(t, global_dir, ServerAction::Restore(id), &["restored,"]).await
        }
        Some(BackupCmd::Delete { id }) => act_remote(t, global_dir, ServerAction::DeleteBackup(id), &["deleted backup"]).await,
        None => {
            let (hub, server, _) = remote(t, global_dir).await?;
            let v = hub.console(&server.id, None, 0).await?;
            if !v.online {
                bail!("{} is offline", server.name);
            }
            if v.backups.is_empty() {
                ui::info("No backups yet. Make one with `beacon server backups create`.");
            }
            for b in &v.backups {
                // The id is what `restore` takes; mod backups' notes are just their path.
                let what = if b.full { format!("whole server · {:<16}", b.note) } else { format!("{:<31}", "world") };
                println!("{:<14} {:>8}  {:<10} {}  {}", b.source, size(b.size), ui::ago(b.created), ui::dim(what), b.id);
            }
            Ok(())
        }
    }
}
