//! Prism Launcher compatible instances: `instance.cfg`, `mmc-pack.json`, `minecraft/`.
//! BeaconSync stores instances inside Prism's instance folder when Prism is installed, so
//! the same folder is playable from Prism and versioned by BeaconSync.

use crate::model::{Loader, LoaderKind};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// Directory where BeaconSync stores its own data (instances when Prism is missing, Java...).
pub fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("beaconsync")
}

/// Prism's data directory, if Prism is installed (native or flatpak).
pub fn prism_data_dir() -> Option<PathBuf> {
    let mut candidates = vec![];
    if let Some(d) = dirs::data_dir() {
        candidates.push(d.join("PrismLauncher"));
    }
    if let Some(h) = dirs::home_dir() {
        candidates.push(h.join(".var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher"));
    }
    candidates.into_iter().find(|d| d.join("prismlauncher.cfg").is_file() || d.join("instances").is_dir())
}

/// Prism's instance folder, honoring a custom `InstanceDir` in `prismlauncher.cfg`.
pub fn prism_instances_dir() -> Option<PathBuf> {
    let data = prism_data_dir()?;
    let cfg = fs::read_to_string(data.join("prismlauncher.cfg")).unwrap_or_default();
    let custom = ini_get(&cfg, "InstanceDir").filter(|v| !v.is_empty());
    Some(match custom {
        Some(dir) if Path::new(&dir).is_absolute() => PathBuf::from(dir),
        Some(dir) => data.join(dir),
        None => data.join("instances"),
    })
}

/// Where new instances are created.
pub fn default_instances_dir(override_dir: Option<&Path>) -> PathBuf {
    override_dir
        .map(Path::to_path_buf)
        .or_else(prism_instances_dir)
        .unwrap_or_else(|| data_dir().join("instances"))
}

fn ini_get(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().to_string())
    })
}

/// Game directory inside an instance (`minecraft/`, or legacy `.minecraft/`).
pub fn game_dir(instance: &Path) -> PathBuf {
    let legacy = instance.join(".minecraft");
    let modern = instance.join("minecraft");
    if legacy.is_dir() && !modern.is_dir() { legacy } else { modern }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InstanceInfo {
    /// Folder name; Prism uses it as the instance id.
    pub id: String,
    pub path: PathBuf,
    pub name: String,
    pub minecraft: String,
    pub loader: Loader,
    /// True when the folder has a `.beacon` repo.
    pub tracked: bool,
}

const LOADER_UIDS: &[(&str, LoaderKind)] = &[
    ("net.fabricmc.fabric-loader", LoaderKind::Fabric),
    ("org.quiltmc.quilt-loader", LoaderKind::Quilt),
    ("net.minecraftforge", LoaderKind::Forge),
    ("net.neoforged", LoaderKind::Neoforge),
];

/// Reads Minecraft version and loader from `mmc-pack.json`.
pub fn read_components(instance: &Path) -> Result<(String, Loader)> {
    let text = fs::read_to_string(instance.join("mmc-pack.json")).context("missing mmc-pack.json")?;
    let v: serde_json::Value = serde_json::from_str(&text)?;
    let mut mc = String::new();
    let mut loader = Loader::default();
    for c in v["components"].as_array().into_iter().flatten() {
        let uid = c["uid"].as_str().unwrap_or_default();
        let version = c["version"].as_str().map(str::to_string);
        if uid == "net.minecraft" {
            mc = version.unwrap_or_default();
        } else if let Some((_, kind)) = LOADER_UIDS.iter().find(|(u, _)| *u == uid) {
            loader = Loader { kind: *kind, version };
        }
    }
    if mc.is_empty() {
        bail!("mmc-pack.json has no Minecraft component");
    }
    Ok((mc, loader))
}

/// Writes `mmc-pack.json`. Prism resolves dependent components (intermediary etc.) itself.
pub fn write_components(instance: &Path, minecraft: &str, loader: &Loader) -> Result<()> {
    let mut components = vec![json!({ "uid": "net.minecraft", "version": minecraft, "important": true })];
    if let Some((uid, _)) = LOADER_UIDS.iter().find(|(_, k)| *k == loader.kind) {
        components.push(json!({ "uid": uid, "version": loader.version }));
    }
    let doc = json!({ "components": components, "formatVersion": 1 });
    fs::write(instance.join("mmc-pack.json"), serde_json::to_string_pretty(&doc)?)?;
    Ok(())
}

pub fn read_name(instance: &Path) -> Option<String> {
    ini_get(&fs::read_to_string(instance.join("instance.cfg")).ok()?, "name")
}

pub fn set_name(instance: &Path, name: &str) -> Result<()> {
    let path = instance.join("instance.cfg");
    let text = fs::read_to_string(&path).unwrap_or_else(|_| "[General]\n".into());
    let mut found = false;
    let mut out: Vec<String> = text
        .lines()
        .map(|l| match l.split_once('=') {
            Some((k, _)) if k.trim() == "name" => {
                found = true;
                format!("name={name}")
            }
            _ => l.to_string(),
        })
        .collect();
    if !found {
        out.push(format!("name={name}"));
    }
    fs::write(path, out.join("\n") + "\n")?;
    Ok(())
}

/// Turns a display name into a safe, unique folder name inside `root`.
pub fn unique_dir(root: &Path, name: &str) -> PathBuf {
    let slug: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() { "pack".to_string() } else { slug };
    let mut dir = root.join(&slug);
    let mut n = 2;
    while dir.exists() {
        dir = root.join(format!("{slug}-{n}"));
        n += 1;
    }
    dir
}

/// Creates an empty instance folder.
pub fn create(dir: &Path, name: &str, minecraft: &str, loader: &Loader) -> Result<()> {
    fs::create_dir_all(dir.join("minecraft/mods"))?;
    fs::write(
        dir.join("instance.cfg"),
        format!("[General]\nConfigVersion=1.2\nInstanceType=OneSix\niconKey=default\nname={name}\n"),
    )?;
    write_components(dir, minecraft, loader)
}

pub fn info(dir: &Path) -> Result<InstanceInfo> {
    let (minecraft, loader) = read_components(dir)?;
    let id = dir.file_name().context("bad dir")?.to_string_lossy().into_owned();
    Ok(InstanceInfo {
        name: read_name(dir).unwrap_or_else(|| id.clone()),
        id,
        path: dir.to_path_buf(),
        minecraft,
        loader,
        tracked: dir.join(".beacon/config.json").is_file(),
    })
}

/// Every instance in the given roots, tracked ones first.
pub fn list(roots: &[PathBuf]) -> Vec<InstanceInfo> {
    let mut out: Vec<InstanceInfo> = roots
        .iter()
        .filter_map(|r| fs::read_dir(r).ok())
        .flatten()
        .flatten()
        .filter(|e| e.path().join("instance.cfg").is_file())
        .filter_map(|e| info(&e.path()).ok())
        .collect();
    out.sort_by(|a, b| b.tracked.cmp(&a.tracked).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out.dedup_by(|a, b| a.path == b.path);
    out
}

/// Finds a Prism executable on this machine.
pub fn prism_command(override_path: Option<&Path>) -> Option<Command> {
    if let Some(p) = override_path {
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

/// Launches an instance through Prism (Prism handles accounts, Java and game files),
/// optionally joining a server (`host:port`) once the game is up.
pub fn launch(instance: &Path, prism_override: Option<&Path>, server: Option<&str>) -> Result<()> {
    let id = instance.file_name().context("bad instance path")?;
    let in_prism = prism_instances_dir().is_some_and(|d| instance.parent() == Some(d.as_path()));
    let mut cmd = prism_command(prism_override).context("Prism Launcher was not found. Install it from prismlauncher.org")?;
    if !in_prism {
        bail!("This instance is outside Prism's instance folder. Move it there or set the instance folder in Settings.");
    }
    cmd.arg("--launch").arg(id);
    if let Some(s) = server {
        cmd.arg("--server").arg(s);
    }
    cmd.spawn().context("starting Prism Launcher")?;
    Ok(())
}
