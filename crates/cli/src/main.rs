//! `beacon`: the BeaconSync command line.
//!
//! With no command it opens the full screen UI (`tui.rs`). Pack commands work on the
//! instance in the current folder (or `-C <dir>`), like git.
//! `beacon server ...` installs and runs auto-updating Minecraft servers.

mod account;
mod backups;
mod console;
mod pack;
mod server;
mod tui;
mod ui;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "beacon", version, about = "BeaconSync: git-like modpacks and auto-updating servers")]
struct Cli {
    /// Instance folder to work on (defaults to the current folder).
    #[arg(short = 'C', long, global = true)]
    dir: Option<PathBuf>,
    /// Omit to open the full screen UI.
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Sign in to a BeaconSync hub (opens a device code flow by default).
    Login {
        /// Hub URL, e.g. https://hub.example.com
        #[arg(long)]
        hub: Option<String>,
        /// Sign in with username and password instead of a device code.
        #[arg(long)]
        password: bool,
    },
    /// Forget the saved hub token.
    Logout,
    /// Show the signed in account.
    Whoami,
    /// List your packs on the hub.
    Packs,

    /// Create a new pack instance.
    New {
        name: String,
        /// Minecraft version, e.g. 1.21.1
        #[arg(long)]
        mc: String,
        /// vanilla, fabric, quilt, forge or neoforge (optionally `fabric@0.16.9`)
        #[arg(long, default_value = "fabric")]
        loader: String,
    },
    /// Start tracking an existing instance folder.
    Init,
    /// Import a .mrpack, CurseForge zip or Prism export, or `--modrinth <slug>` to install a
    /// Modrinth modpack.
    Import {
        file: Option<PathBuf>,
        #[arg(long, conflicts_with = "file")]
        modrinth: Option<String>,
    },
    /// Join a published pack by share code or link (use --fork for an editable copy).
    Join {
        code: String,
        #[arg(long)]
        fork: bool,
    },
    /// Show unsaved changes.
    Status,
    /// Save the current files as a new version.
    #[command(alias = "commit")]
    Save {
        #[arg(short, long)]
        message: String,
    },
    /// List saved versions.
    Log {
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: usize,
    },
    /// Throw away unsaved changes.
    Discard,
    /// Put an older version's files back (save afterwards to keep the rollback).
    Restore { rev: String },
    /// Move the branch back to a version, dropping newer ones.
    Reset { rev: String },
    /// Roll back to a version for everyone: save it as a new version, publish and deploy it.
    Rollback {
        rev: String,
        /// Only roll back locally; don't publish or deploy.
        #[arg(long)]
        local: bool,
    },
    /// List, create (`beacon branch name`) or delete (`-d`) branches.
    Branch {
        name: Option<String>,
        #[arg(short, long)]
        delete: bool,
    },
    /// Switch to a branch or version.
    Switch {
        target: String,
        #[arg(long)]
        force: bool,
    },
    /// Merge another branch into the current one.
    Merge { branch: String },
    /// Add a mod, resource pack or shader from Modrinth (slug or id) with its dependencies.
    Add { project: String },
    /// Check Modrinth for newer versions of your mods (`--apply` installs them).
    Updates {
        #[arg(long)]
        apply: bool,
    },
    /// Check or pull changes from the pack this one was forked or imported from.
    Upstream {
        #[arg(long)]
        pull: bool,
    },
    /// Push versions to the hub.
    Push {
        #[arg(long)]
        force: bool,
    },
    /// Bring in versions saved on the hub from another machine.
    Pull,
    /// Publish the current version as a release players and servers can follow.
    Publish {
        name: String,
        #[arg(short, long, default_value = "")]
        notes: String,
    },
    /// Deploy a release to servers (latest release and all servers by default).
    Deploy {
        /// Release name or id.
        #[arg(long)]
        release: Option<String>,
        /// Server ids or names (repeatable). Omit for all servers.
        #[arg(long = "server")]
        servers: Vec<String>,
    },
    /// Install the newest release of a joined pack.
    Update,
    /// Export the pack as .mrpack or as a server pack zip.
    Export {
        #[arg(value_enum)]
        kind: pack::ExportKind,
        out: PathBuf,
    },
    /// Launch the instance with Prism Launcher.
    Play,
    /// Install, run and manage Minecraft servers.
    Server {
        #[command(subcommand)]
        cmd: server::ServerCmd,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_env("BEACON_LOG").unwrap_or_else(|_| "warn".into()))
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    if let Err(e) = run(cli).await {
        ui::warn(format!("{e:#}"));
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let dir = cli.dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let Some(cmd) = cli.cmd else {
        use std::io::IsTerminal;
        if !std::io::stdout().is_terminal() {
            <Cli as clap::CommandFactory>::command().print_help()?;
            return Ok(());
        }
        return tui::run().await;
    };
    match cmd {
        Cmd::Login { hub, password } => account::login(hub, password).await,
        Cmd::Logout => account::logout(),
        Cmd::Whoami => account::whoami().await,
        Cmd::Packs => account::packs().await,
        Cmd::New { name, mc, loader } => pack::new(&name, &mc, &loader).await,
        Cmd::Init => pack::init(&dir).await,
        Cmd::Import { file, modrinth } => pack::import(file, modrinth).await,
        Cmd::Join { code, fork } => pack::join(&code, fork).await,
        Cmd::Status => pack::status(&dir),
        Cmd::Save { message } => pack::save(&dir, &message).await,
        Cmd::Log { limit } => pack::log(&dir, limit),
        Cmd::Discard => pack::discard(&dir).await,
        Cmd::Restore { rev } => pack::restore(&dir, &rev).await,
        Cmd::Reset { rev } => pack::reset(&dir, &rev).await,
        Cmd::Rollback { rev, local } => pack::rollback(&dir, &rev, local).await,
        Cmd::Branch { name, delete } => pack::branch(&dir, name, delete),
        Cmd::Switch { target, force } => pack::switch(&dir, &target, force).await,
        Cmd::Merge { branch } => pack::merge(&dir, &branch).await,
        Cmd::Add { project } => pack::add(&dir, &project).await,
        Cmd::Updates { apply } => pack::updates(&dir, apply).await,
        Cmd::Upstream { pull } => pack::upstream(&dir, pull).await,
        Cmd::Push { force } => pack::push(&dir, force).await,
        Cmd::Pull => pack::pull(&dir).await,
        Cmd::Publish { name, notes } => pack::publish(&dir, &name, &notes).await,
        Cmd::Deploy { release, servers } => pack::deploy(&dir, release, servers).await,
        Cmd::Update => pack::update(&dir).await,
        Cmd::Export { kind, out } => pack::export(&dir, kind, &out).await,
        Cmd::Play => pack::play(&dir).await,
        Cmd::Server { cmd } => server::run(cmd, cli.dir).await,
    }
}
