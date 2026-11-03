//! `beacon server`: install a pack's server, supervise it and apply deployments.
//!
//! `beacon server run` is the heart of service and docker modes: it runs Minecraft as a child
//! process, restarts it when it crashes, long polls the hub and applies new releases
//! (announce, stop, back up, update, restart, report).

use crate::ui::{self, Bar};
use anyhow::{Context, Result, bail};
use beacon_core::{
    api::{AgentReport, AgentTarget, NewServer, ServerMode},
    fetch::{Fetcher, Progress},
    hash::short,
    hub::Hub,
    model::{Commit, Tree},
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
        #[arg(long, value_enum)]
        mode: Option<ModeArg>,
        /// Folder to install into.
        #[arg(long)]
        path: Option<PathBuf>,
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
        /// Do not start the server after installing.
        #[arg(long)]
        no_start: bool,
    },
    /// Run the server: supervise Minecraft and apply deployments automatically.
    Run { path: Option<PathBuf> },
    /// Apply the latest deployment once (for plain mode servers, stop Minecraft first).
    Update { path: Option<PathBuf> },
    /// Show what this server is running.
    Status { path: Option<PathBuf> },
    /// Go back to the backup taken before the last update.
    Rollback { path: Option<PathBuf> },
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

pub async fn run(cmd: ServerCmd, global_dir: Option<PathBuf>) -> Result<()> {
    let pick = |p: Option<PathBuf>| -> Result<PathBuf> {
        let d = p.or(global_dir.clone()).unwrap_or(std::env::current_dir()?);
        Ok(d.canonicalize().unwrap_or(d))
    };
    match cmd {
        ServerCmd::Install { pack, mode, path, name, address, memory, accept_eula, no_start } => {
            install(InstallArgs { pack, mode: mode.map(Into::into), path, name, address, memory, accept_eula, no_start }).await
        }
        ServerCmd::Run { path } => supervise(pick(path)?).await,
        ServerCmd::Update { path } => update_once(&pick(path)?).await,
        ServerCmd::Status { path } => status(&pick(path)?).await,
        ServerCmd::Rollback { path } => rollback(&pick(path)?).await,
    }
}

fn log_progress(p: Progress) {
    if p.done == p.total || p.total <= 1 {
        println!("[beacon] {} ({}/{})", p.label, p.done, p.total);
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

/// Half the machine's RAM, clamped to 2-8 GB.
fn default_memory_mb() -> u32 {
    let total_kb: u64 = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("MemTotal:")).and_then(|l| l.split_whitespace().nth(1)?.parse().ok()))
        .unwrap_or(8 * 1024 * 1024);
    let half = (total_kb / 1024 / 2) as u32;
    (half.clamp(2048, 8192) / 512) * 512
}

fn slug(s: &str) -> String {
    let s: String = s.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    s.trim_matches('-').to_string()
}

fn agent_hub(cfg: &ServerConfig) -> Option<Hub> {
    Some(Hub::new(cfg.hub.as_deref()?, Some(cfg.token.clone()?)))
}

async fn report(hub: &Option<Hub>, deployment: Option<String>, status: &str, message: impl Into<String>, commit: Option<String>) {
    if let Some(h) = hub
        && let Err(e) = h.agent_report(&AgentReport { deployment, status: status.into(), message: message.into(), commit }).await
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
            let url = format!(
                "{}/beacon-{}-unknown-linux-musl",
                beacon_core::companion::RELEASES,
                std::env::consts::ARCH
            );
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

struct InstallArgs {
    pack: Option<String>,
    mode: Option<ServerMode>,
    path: Option<PathBuf>,
    name: Option<String>,
    address: Option<String>,
    memory: Option<u32>,
    accept_eula: bool,
    no_start: bool,
}

async fn install(a: InstallArgs) -> Result<()> {
    let settings = Settings::load();
    let hub = Hub::authed(&settings).context("run `beacon login` first")?;
    let packs: Vec<_> = hub.library().await?.into_iter().filter(|p| p.mine).collect();
    if packs.is_empty() {
        bail!("you have no published packs; publish one from the app first");
    }
    let pack = match &a.pack {
        Some(q) => packs
            .iter()
            .find(|p| p.code.eq_ignore_ascii_case(q) || &p.id == q || p.name.eq_ignore_ascii_case(q))
            .with_context(|| format!("no pack {q} in your library"))?,
        None => {
            let items: Vec<String> = packs.iter().map(|p| format!("{}  {} {} · {}", p.name, p.loader.kind.label(), p.minecraft, p.code)).collect();
            &packs[ui::select("Which pack?", &items, 0)?]
        }
    };
    let latest = pack.latest.clone().context("this pack has no release yet; publish one first")?;
    let mode = match a.mode {
        Some(m) => m,
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
    let name = match a.name {
        Some(n) => n,
        None => ui::input("Server name", Some(format!("{} on {}", pack.name, hostname())))?,
    };
    let address = match a.address {
        Some(x) => x,
        None => ui::input("Address players connect to", Some(format!("{}:25565", hostname())))?,
    };
    let dir = match a.path {
        Some(p) => p,
        None => PathBuf::from(ui::input("Install folder", Some(std::env::current_dir()?.join(slug(&pack.name)).to_string_lossy().into_owned()))?),
    };
    let memory_mb = match a.memory {
        Some(m) => m,
        None => ui::input("Memory for Minecraft (MB)", Some(default_memory_mb().to_string()))?.parse().context("not a number")?,
    };
    if ServerConfig::path(&dir).exists() {
        bail!("{} already has a BeaconSync server; use `beacon server update`", dir.display());
    }
    if !a.accept_eula && !ui::confirm("Do you accept the Minecraft EULA (https://aka.ms/MinecraftEULA)?", true)? {
        bail!("the server can't start without accepting the EULA");
    }
    fs::create_dir_all(&dir)?;
    let dir = dir.canonicalize()?;

    ui::info(format!("Registering {name} with the hub"));
    let created = hub.create_server(&pack.id, &NewServer { name: name.clone(), mode, address: address.clone() }).await?;
    let mut cfg = ServerConfig {
        format: 1,
        name: name.clone(),
        mode,
        address,
        hub: Some(hub.base.clone()),
        pack_code: Some(pack.code.clone()),
        server_id: Some(created.server.id.clone()),
        token: Some(created.token.clone()),
        memory_mb,
        jvm_args: vec!["-XX:+UseG1GC".into(), "-XX:+ParallelRefProcEnabled".into(), "-XX:MaxGCPauseMillis=200".into()],
        java: PathBuf::from("java"),
        start: vec![],
        commit: None,
        release_name: None,
        installed: None,
    };
    cfg.save(&dir)?;

    let store = ObjectStore::new(dir.join(".beacon/objects"));
    let tree = fetch_tree(&hub, &store, &latest.commit).await?;
    let bar = Bar::new();
    let fetcher = Fetcher::new(Some(store), Some(hub.base.clone()));
    srv::apply_tree(&dir, &tree, &fetcher, &|p| bar.update(p)).await?;
    bar.done();
    srv::accept_eula(&dir)?;
    let beacon = install_self(&dir, mode).await?;
    if mode != ServerMode::Docker {
        let bar = Bar::new();
        srv::ensure_loader(&dir, &mut cfg, &tree, &|p| bar.update(p)).await?;
        bar.done();
        srv::write_start_scripts(&dir, &cfg)?;
    }
    cfg.commit = Some(latest.commit.clone());
    cfg.release_name = Some(latest.name.clone());
    cfg.save(&dir)?;
    let agent = agent_hub(&cfg);
    report(&agent, None, "idle", format!("installed {}", latest.name), cfg.commit.clone()).await;
    ui::ok(format!("Installed {} {} in {}", pack.name, latest.name, dir.display()));

    match mode {
        ServerMode::Plain => {
            println!();
            let script = if cfg!(windows) { "start.bat" } else { "./start.sh" };
            println!("  Start it:      cd {} && {script}", dir.display());
            println!("  Auto updates:  {} server run {}", beacon.display(), dir.display());
        }
        ServerMode::Service => setup_service(&cfg, &beacon, &dir, a.no_start)?,
        ServerMode::Docker => setup_docker(&cfg, &tree, &dir, a.no_start)?,
    }
    Ok(())
}

fn run_cmd(program: &str, args: &[&str], dir: &Path) -> Result<()> {
    let status = std::process::Command::new(program).args(args).current_dir(dir).status().with_context(|| format!("running {program}"))?;
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

fn setup_service(cfg: &ServerConfig, beacon: &Path, dir: &Path, no_start: bool) -> Result<()> {
    let name = srv::service_name(cfg);
    let unit_file = dir.join(format!("{name}.service"));
    fs::write(&unit_file, srv::systemd_unit(cfg, beacon, dir, &current_user()))?;
    let target = format!("/etc/systemd/system/{name}.service");
    let unit = unit_file.to_string_lossy().into_owned();
    let sudo = |args: &[&str]| -> Result<()> {
        if is_root() { run_cmd(args[0], &args[1..], dir) } else { run_cmd("sudo", args, dir) }
    };
    if !is_root() {
        ui::info("Installing the systemd service needs sudo");
    }
    sudo(&["install", "-m", "644", &unit, &target])?;
    sudo(&["systemctl", "daemon-reload"])?;
    if no_start {
        sudo(&["systemctl", "enable", &name])?;
    } else {
        sudo(&["systemctl", "enable", "--now", &name])?;
    }
    ui::ok(format!("Service {name} {}", if no_start { "enabled" } else { "started" }));
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

fn setup_docker(cfg: &ServerConfig, tree: &Tree, dir: &Path, no_start: bool) -> Result<()> {
    fs::create_dir_all(dir.join(".beacon/home"))?;
    let java = beacon_core::meta::java_for(&tree.minecraft);
    fs::write(dir.join("docker-compose.yml"), srv::docker_compose(cfg, java, uid_gid()))?;
    ui::ok("Wrote docker-compose.yml");
    if !no_start {
        run_cmd("docker", &["compose", "up", "-d"], dir)?;
        ui::ok("Container started (first start installs the loader, give it a minute)");
    }
    println!("  Logs:     docker compose -f {} logs -f", dir.join("docker-compose.yml").display());
    println!("  Console:  docker attach beacon-{}", slug(&cfg.name));
    Ok(())
}

// ---- supervisor ---------------------------------------------------------------------------

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

impl Mc {
    fn start(dir: &Path, cfg: &ServerConfig) -> Result<Self> {
        let argv = cfg.argv();
        println!("[beacon] starting: {}", argv.join(" "));
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..]).current_dir(dir).stdin(Stdio::piped()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
        // Own process group so Ctrl+C reaches us, and we stop Minecraft cleanly.
        #[cfg(unix)]
        cmd.process_group(0);
        let mut child = cmd.spawn().context("starting Minecraft")?;
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
            println!("[beacon] Minecraft did not stop in 90s, killing it");
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

async fn agent_loop(hub: Hub, tx: mpsc::Sender<AgentTarget>) {
    let mut last: Option<String> = None;
    loop {
        match hub.agent_poll(25).await {
            Ok(p) => {
                if let Some(t) = p.target
                    && last.as_ref() != Some(&t.deployment)
                {
                    last = Some(t.deployment.clone());
                    if tx.send(t).await.is_err() {
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
    if let Some(h) = agent.clone() {
        tokio::spawn(agent_loop(h, tx_target));
    }
    let (tx_in, mut console) = mpsc::channel(16);
    tokio::spawn(stdin_loop(tx_in));
    #[cfg(unix)]
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    let mut mc = Some(Mc::start(&dir, &cfg)?);
    report(&agent, None, "running", format!("running {}", cfg.release_name.clone().unwrap_or_default()), cfg.commit.clone()).await;
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
                println!("[beacon] Minecraft exited ({code}); restarting in {wait}s");
                report(&agent, None, "failed", format!("Minecraft exited ({code}), restarting"), cfg.commit.clone()).await;
                tokio::time::sleep(Duration::from_secs(wait)).await;
                mc = Some(Mc::start(&dir, &cfg)?);
                report(&agent, None, "running", "restarted after a crash", cfg.commit.clone()).await;
            }
            Some(target) = targets.recv() => {
                let hub = agent.clone().context("no hub")?;
                apply_update(&dir, &mut cfg, &store, &hub, target, mc.take()).await;
                mc = Some(Mc::start(&dir, &cfg)?);
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
    println!("[beacon] stopping Minecraft");
    if let Some(m) = mc {
        m.stop().await;
    }
    report(&agent, None, "stopped", "server stopped", cfg.commit.clone()).await;
    Ok(())
}

/// Announces, stops Minecraft, backs up, applies the new version and reports. On failure
/// the backup is restored so the server comes back on the previous version.
async fn apply_update(dir: &Path, cfg: &mut ServerConfig, store: &ObjectStore, hub: &Hub, t: AgentTarget, mc: Option<Mc>) {
    let agent = Some(hub.clone());
    let dep = Some(t.deployment.clone());
    println!("[beacon] deploying {} ({})", t.release_name, short(&t.commit));
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
            println!("[beacon] now on {}", t.release_name);
            report(&agent, dep, "done", format!("running {}", t.release_name), cfg.commit.clone()).await;
        }
        Err(e) => {
            println!("[beacon] update failed: {e:#}");
            report(&agent, dep, "failed", format!("update failed: {e:#}"), cfg.commit.clone()).await;
        }
    }
}

async fn apply_with_backup(dir: &Path, cfg: &mut ServerConfig, store: &ObjectStore, hub: &Hub, tree: &Tree, t: &AgentTarget) -> Result<()> {
    let d = dir.to_path_buf();
    tokio::task::spawn_blocking(move || backup(&d)).await??;
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
        let d = dir.to_path_buf();
        tokio::task::spawn_blocking(move || restore(&d)).await??;
        *cfg = ServerConfig::load(dir)?;
        return Err(e);
    }
    cfg.commit = Some(t.commit.clone());
    cfg.release_name = Some(t.release_name.clone());
    cfg.save(dir)
}

fn backup_path(dir: &Path) -> PathBuf {
    dir.join(".beacon/backup.tar.gz")
}

fn skip_in_backup(rel: &Path) -> bool {
    let s = rel.to_string_lossy();
    s.starts_with(".beacon/backup") || s.starts_with(".beacon/objects") || s.starts_with(".fabric") || s.starts_with(".beacon/bin") || s.starts_with(".beacon/home")
        || s.starts_with("logs") || s.starts_with("crash-reports") || s.ends_with(".part")
}

/// Replaces the single backup with a fresh one of the whole server (worlds included).
fn backup(dir: &Path) -> Result<()> {
    println!("[beacon] backing up");
    let tmp = dir.join(".beacon/backup.tar.gz.tmp");
    {
        let enc = flate2::write::GzEncoder::new(fs::File::create(&tmp)?, flate2::Compression::fast());
        let mut tar = tar::Builder::new(enc);
        tar.follow_symlinks(false);
        for e in walkdir::WalkDir::new(dir).min_depth(1).into_iter().filter_entry(|e| !skip_in_backup(e.path().strip_prefix(dir).unwrap_or(e.path()))) {
            let e = e?;
            if e.file_type().is_file() {
                tar.append_path_with_name(e.path(), e.path().strip_prefix(dir)?)?;
            }
        }
        tar.into_inner()?.finish()?;
    }
    fs::rename(&tmp, backup_path(dir))?;
    Ok(())
}

/// Removes pack managed files, then unpacks the backup over the server folder.
fn restore(dir: &Path) -> Result<()> {
    let file = backup_path(dir);
    if !file.is_file() {
        bail!("no backup to restore");
    }
    println!("[beacon] restoring backup");
    if let Some(tree) = srv::applied_tree(dir) {
        for p in tree.files.keys() {
            fs::remove_file(dir.join(p)).ok();
        }
    }
    tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(file)?)).unpack(dir)?;
    Ok(())
}

async fn update_once(dir: &Path) -> Result<()> {
    if running_pid(dir).is_some() {
        bail!("this server runs under `beacon server run`, which applies updates automatically");
    }
    let mut cfg = ServerConfig::load(dir)?;
    let hub = agent_hub(&cfg).context("server is not linked to a hub")?;
    let poll = hub.agent_poll(0).await?;
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
    println!("  Version   {} {}", cfg.release_name.as_deref().unwrap_or("?"), ui::dim(cfg.commit.as_deref().map(short).unwrap_or("")));
    if let Some((mc, l)) = &cfg.installed {
        println!("  Minecraft {mc} {} {}", l.kind.label(), l.version.as_deref().unwrap_or(""));
    }
    println!("  Running   {}", if running_pid(dir).is_some() { "yes" } else { "no (or not supervised)" });
    println!("  Backup    {}", if backup_path(dir).is_file() { "yes" } else { "none yet" });
    if let Some(hub) = agent_hub(&cfg) {
        match hub.agent_poll(0).await {
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
        bail!("stop the server first (e.g. `sudo systemctl stop {}`)", srv::service_name(&ServerConfig::load(dir)?));
    }
    if !ui::confirm("Restore the backup from before the last update? World changes since then are lost.", false)? {
        return Ok(());
    }
    let d = dir.to_path_buf();
    tokio::task::spawn_blocking(move || restore(&d)).await??;
    let cfg = ServerConfig::load(dir)?;
    // Reported as failed so the hub doesn't immediately push the same release again.
    report(&agent_hub(&cfg), None, "failed", format!("rolled back to {}", cfg.release_name.clone().unwrap_or_default()), cfg.commit.clone()).await;
    ui::ok(format!("Rolled back to {}. Start the server again.", cfg.release_name.unwrap_or_default()));
    Ok(())
}
