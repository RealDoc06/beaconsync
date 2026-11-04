//! Server packs: picking server side files, installing the loader, writing start scripts,
//! docker and systemd files, and applying new versions to an installed server.
//!
//! An installed server keeps its state in `<dir>/.beacon/server.json` and the applied tree in
//! `<dir>/.beacon/server-tree.json`.

pub mod java;

use crate::{
    api::ServerMode,
    fetch::{Fetcher, OnProgress, Progress},
    hash,
    model::{Loader, LoaderKind, Tree, client_only_path},
    store::write_atomic,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServerConfig {
    pub format: u32,
    pub name: String,
    pub mode: ServerMode,
    pub address: String,
    pub hub: Option<String>,
    pub pack_code: Option<String>,
    pub server_id: Option<String>,
    /// Server scoped hub token (only valid for the agent endpoints).
    pub token: Option<String>,
    pub memory_mb: u32,
    #[serde(default)]
    pub jvm_args: Vec<String>,
    /// Java executable; plain `java` inside docker.
    pub java: PathBuf,
    /// Arguments after the JVM flags, e.g. `-jar fabric-server-launch.jar nogui`.
    pub start: Vec<String>,
    pub commit: Option<String>,
    pub release_name: Option<String>,
    /// `(minecraft, loader)` the start command was installed for.
    pub installed: Option<(String, Loader)>,
}

impl ServerConfig {
    pub fn path(dir: &Path) -> PathBuf {
        dir.join(".beacon/server.json")
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let p = Self::path(dir);
        let bytes = fs::read(&p).with_context(|| format!("{} is not a BeaconSync server (no {})", dir.display(), p.display()))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        let p = Self::path(dir);
        write_atomic(&p, &serde_json::to_vec_pretty(self)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    /// Full command line used to start Minecraft.
    pub fn argv(&self) -> Vec<String> {
        let mem = format!("{}M", self.memory_mb);
        let mut v = vec![self.java.to_string_lossy().into_owned(), format!("-Xms{mem}"), format!("-Xmx{mem}")];
        v.extend(self.jvm_args.iter().cloned());
        v.extend(self.start.iter().cloned());
        v
    }
}

/// Only what a dedicated server needs: no client only mods, resource packs or shaders.
pub fn server_tree(tree: &Tree) -> Tree {
    let mut t = tree.clone();
    t.files.retain(|p, e| e.side.on_server() && !client_only_path(p) && !crate::model::is_disabled(p));
    t
}

fn applied_tree_path(dir: &Path) -> PathBuf {
    dir.join(".beacon/server-tree.json")
}

pub fn applied_tree(dir: &Path) -> Option<Tree> {
    serde_json::from_slice(&fs::read(applied_tree_path(dir)).ok()?).ok()
}

/// Brings the server's pack files in line with `tree` (server side files only), leaving
/// worlds and anything not managed by the pack alone.
pub async fn apply_tree(dir: &Path, tree: &Tree, fetcher: &Fetcher, on: OnProgress<'_>) -> Result<()> {
    tree.validate()?;
    let new = server_tree(tree);
    let old = applied_tree(dir).unwrap_or_default();
    let mut fetch = vec![];
    for (path, e) in &new.files {
        let dest = dir.join(path);
        let unchanged = old.files.get(path).is_some_and(|o| o.hash == e.hash) && dest.is_file();
        let on_disk = !unchanged && dest.is_file() && hash::sha512_file(&dest).is_ok_and(|h| h == e.hash);
        if !unchanged && !on_disk {
            fetch.push((e.clone(), dest));
        }
    }
    fetcher.fetch_many(fetch, on).await?;
    for path in old.files.keys().filter(|p| !new.files.contains_key(*p)) {
        fs::remove_file(dir.join(path)).ok();
    }
    write_atomic(&applied_tree_path(dir), &serde_json::to_vec(&new)?)
}

async fn get_json(url: &str) -> Result<serde_json::Value> {
    Ok(crate::http().get(url).send().await?.error_for_status()?.json().await?)
}

async fn get_to(url: &str, dest: &Path) -> Result<()> {
    let bytes = crate::http().get(url).send().await?.error_for_status()?.bytes().await?;
    write_atomic(dest, &bytes)
}

fn run_installer(java: &Path, dir: &Path, args: &[&str]) -> Result<()> {
    let out = Command::new(java).args(args).current_dir(dir).output().context("running loader installer")?;
    if !out.status.success() {
        let tail: String = String::from_utf8_lossy(&out.stdout).lines().rev().take(15).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
        bail!("loader installer failed:\n{tail}");
    }
    Ok(())
}

/// Installs the server side of Minecraft + loader into `dir` and returns the start arguments.
pub async fn install_loader(dir: &Path, minecraft: &str, loader: &Loader, java: &Path, on: OnProgress<'_>) -> Result<Vec<String>> {
    let label = format!("Installing {} server {minecraft}", loader.kind.label());
    on(Progress { done: 0, total: 1, label: label.clone() });
    let v = loader.version.as_deref().unwrap_or_default();
    let start = match loader.kind {
        LoaderKind::Vanilla => {
            let manifest = get_json("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").await?;
            let url = manifest["versions"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|x| x["id"] == minecraft)
                .and_then(|x| x["url"].as_str())
                .context("unknown Minecraft version")?
                .to_string();
            let meta = get_json(&url).await?;
            let jar = meta["downloads"]["server"]["url"].as_str().context("this version has no server jar")?;
            get_to(jar, &dir.join("server.jar")).await?;
            vec!["-jar".into(), "server.jar".into(), "nogui".into()]
        }
        LoaderKind::Fabric => {
            let installers = get_json("https://meta.fabricmc.net/v2/versions/installer").await?;
            let inst = installers[0]["version"].as_str().context("no fabric installer")?;
            get_to(
                &format!("https://meta.fabricmc.net/v2/versions/loader/{minecraft}/{v}/{inst}/server/jar"),
                &dir.join("fabric-server-launch.jar"),
            )
            .await?;
            vec!["-jar".into(), "fabric-server-launch.jar".into(), "nogui".into()]
        }
        LoaderKind::Quilt => {
            let xml = crate::http()
                .get("https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-installer/maven-metadata.xml")
                .send()
                .await?
                .text()
                .await?;
            let inst = xml.split("<release>").nth(1).and_then(|s| s.split("</release>").next()).context("no quilt installer")?;
            let jar = dir.join("quilt-installer.jar");
            get_to(&format!("https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-installer/{inst}/quilt-installer-{inst}.jar"), &jar).await?;
            let d = dir.to_string_lossy().into_owned();
            let (java, dirc) = (java.to_path_buf(), dir.to_path_buf());
            let (mc, lv) = (minecraft.to_string(), v.to_string());
            tokio::task::spawn_blocking(move || {
                run_installer(&java, &dirc, &["-jar", "quilt-installer.jar", "install", "server", &mc, &lv, "--download-server", &format!("--install-dir={d}")])
            })
            .await??;
            fs::remove_file(jar).ok();
            vec!["-jar".into(), "quilt-server-launch.jar".into(), "nogui".into()]
        }
        LoaderKind::Forge | LoaderKind::Neoforge => {
            let (url, lib) = if loader.kind == LoaderKind::Forge {
                let full = format!("{minecraft}-{v}");
                (
                    format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{full}/forge-{full}-installer.jar"),
                    format!("libraries/net/minecraftforge/forge/{full}"),
                )
            } else {
                (
                    format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{v}/neoforge-{v}-installer.jar"),
                    format!("libraries/net/neoforged/neoforge/{v}"),
                )
            };
            let jar = dir.join("loader-installer.jar");
            get_to(&url, &jar).await?;
            let (java, dirc) = (java.to_path_buf(), dir.to_path_buf());
            tokio::task::spawn_blocking(move || run_installer(&java, &dirc, &["-jar", "loader-installer.jar", "--installServer"])).await??;
            fs::remove_file(&jar).ok();
            fs::remove_file(dir.join("loader-installer.jar.log")).ok();
            let args_file = if cfg!(windows) { "win_args.txt" } else { "unix_args.txt" };
            if dir.join(&lib).join(args_file).is_file() {
                vec![format!("@{lib}/{args_file}"), "nogui".into()]
            } else {
                // Pre 1.17 Forge ships a runnable jar instead of an args file.
                let jar = fs::read_dir(dir)?
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .find(|n| n.starts_with("forge-") && n.ends_with(".jar") && !n.contains("installer"))
                    .context("forge installer produced no server jar")?;
                vec!["-jar".into(), jar, "nogui".into()]
            }
        }
    };
    on(Progress { done: 1, total: 1, label });
    Ok(start)
}

/// Writes `start.sh` / `start.bat` for running the server by hand.
pub fn write_start_scripts(dir: &Path, cfg: &ServerConfig) -> Result<()> {
    let quote = |s: &str| if s.contains(' ') { format!("\"{s}\"") } else { s.to_string() };
    let args: Vec<String> = cfg.argv().iter().map(|a| quote(a)).collect();
    let sh = format!("#!/usr/bin/env bash\n# Generated by BeaconSync\ncd \"$(dirname \"$0\")\"\nexec {} \"$@\"\n", args.join(" "));
    write_atomic(&dir.join("start.sh"), sh.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir.join("start.sh"), fs::Permissions::from_mode(0o755))?;
    }
    let bat = format!("@echo off\r\nrem Generated by BeaconSync\r\ncd /d \"%~dp0\"\r\n{}\r\npause\r\n", args.join(" ").replace("unix_args.txt", "win_args.txt"));
    write_atomic(&dir.join("start.bat"), bat.as_bytes())
}

pub fn accept_eula(dir: &Path) -> Result<()> {
    write_atomic(&dir.join("eula.txt"), b"# Accepted through BeaconSync (https://aka.ms/MinecraftEULA)\neula=true\n")
}

fn port_of(address: &str) -> u16 {
    address.rsplit_once(':').and_then(|(_, p)| p.parse().ok()).unwrap_or(25565)
}

/// docker compose file running `beacon server run` inside a Temurin image.
/// The beacon binary is copied to `.beacon/bin/beacon` so the container runs the same agent.
pub fn docker_compose(cfg: &ServerConfig, java_major: u32, user: Option<(u32, u32)>) -> String {
    let slug = slug(&cfg.name);
    format!(
        "# Generated by BeaconSync. Start with: docker compose up -d\nservices:\n  minecraft:\n    image: eclipse-temurin:{java_major}-jre\n    container_name: beacon-{slug}\n    working_dir: /server\n    command: [\"/server/.beacon/bin/beacon\", \"server\", \"run\", \"/server\"]\n    volumes:\n      - ./:/server\n    ports:\n      - \"{port}:{port}\"\n    restart: unless-stopped\n    stdin_open: true\n    tty: true\n    stop_grace_period: 120s\n    environment:\n      HOME: /server/.beacon/home\n{user}",
        port = port_of(&cfg.address),
        user = user.map(|(u, g)| format!("    user: \"{u}:{g}\"\n")).unwrap_or_default(),
    )
}

/// systemd unit for service mode.
pub fn systemd_unit(cfg: &ServerConfig, beacon: &Path, dir: &Path, user: &str) -> String {
    format!(
        "# Generated by BeaconSync\n[Unit]\nDescription=BeaconSync Minecraft server {name}\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nUser={user}\nWorkingDirectory={dir}\nExecStart={beacon} server run {dir}\nRestart=always\nRestartSec=10\nTimeoutStopSec=120\nKillSignal=SIGTERM\n\n[Install]\nWantedBy=multi-user.target\n",
        name = cfg.name,
        dir = dir.display(),
        beacon = beacon.display(),
    )
}

/// Lowercase, dash separated name safe for services, containers and folders.
pub fn slug(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

pub fn service_name(cfg: &ServerConfig) -> String {
    format!("beacon-{}", slug(&cfg.name))
}

/// Installs or updates Minecraft + loader when the tree asks for a different one.
pub async fn ensure_loader(dir: &Path, cfg: &mut ServerConfig, tree: &Tree, on: OnProgress<'_>) -> Result<()> {
    let wanted = (tree.minecraft.clone(), tree.loader.clone());
    if cfg.installed.as_ref() == Some(&wanted) && !cfg.start.is_empty() {
        return Ok(());
    }
    if cfg.mode != ServerMode::Docker || cfg.java != Path::new("java") {
        cfg.java = java::ensure(crate::meta::java_for(&tree.minecraft), on).await?;
    }
    cfg.start = install_loader(dir, &tree.minecraft, &tree.loader, &cfg.java, on).await?;
    cfg.installed = Some(wanted);
    Ok(())
}

/// Builds a standalone server pack folder from a tree (used for "Export server pack").
pub async fn build_folder(dir: &Path, name: &str, tree: &Tree, fetcher: &Fetcher, memory_mb: u32, on: OnProgress<'_>) -> Result<ServerConfig> {
    fs::create_dir_all(dir)?;
    apply_tree(dir, tree, fetcher, on).await?;
    let java_major = crate::meta::java_for(&tree.minecraft);
    let java = java::ensure(java_major, on).await.unwrap_or_else(|_| PathBuf::from("java"));
    let start = install_loader(dir, &tree.minecraft, &tree.loader, &java, on).await?;
    let cfg = ServerConfig {
        format: 1,
        name: name.into(),
        mode: ServerMode::Plain,
        address: "0.0.0.0:25565".into(),
        hub: None,
        pack_code: None,
        server_id: None,
        token: None,
        memory_mb,
        jvm_args: vec![],
        java: PathBuf::from("java"),
        start,
        commit: None,
        release_name: None,
        installed: Some((tree.minecraft.clone(), tree.loader.clone())),
    };
    write_start_scripts(dir, &cfg)?;
    Ok(cfg)
}

/// Zips a folder (used to hand out server packs).
pub fn zip_dir(dir: &Path, out: &Path) -> Result<()> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(fs::File::create(out)?);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for e in walkdir::WalkDir::new(dir).min_depth(1) {
        let e = e?;
        let rel = e.path().strip_prefix(dir)?.to_string_lossy().replace('\\', "/");
        if rel.starts_with(".beacon") {
            continue;
        }
        if e.file_type().is_dir() {
            zip.add_directory(rel, opts)?;
        } else {
            zip.start_file(rel, opts)?;
            zip.write_all(&fs::read(e.path())?)?;
        }
    }
    zip.finish()?;
    Ok(())
}
