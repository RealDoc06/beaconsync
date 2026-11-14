//! Prism Launcher compatible instances: `instance.cfg`, `mmc-pack.json`, `minecraft/`.
//! BeaconSync stores instances inside Prism's instance folder when Prism is installed, so
//! the same folder is playable from Prism and versioned by BeaconSync. Launching is in [`crate::prism`].

use crate::model::{Loader, LoaderKind};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
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
    override_dir.map(Path::to_path_buf).or_else(prism_instances_dir).unwrap_or_else(|| data_dir().join("instances"))
}

fn ini_get(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().to_string())
    })
}

/// `text` (a Prism `.cfg`) with `key` set to `value`. An existing line is replaced in place;
/// a new one goes under `[General]` (Qt INI, what Prism writes) or at the end (the flat format
/// Prism reads too).
pub fn ini_set(text: &str, key: &str, value: &str) -> String {
    let line = format!("{key}={value}");
    let mut lines: Vec<&str> = text.lines().collect();
    match lines.iter().position(|l| l.split_once('=').is_some_and(|(k, _)| k.trim() == key)) {
        Some(i) => lines[i] = &line,
        None => match lines.iter().position(|l| l.trim() == "[General]") {
            Some(i) => lines.insert(i + 1, &line),
            None => lines.push(&line),
        },
    }
    lines.join("\n") + "\n"
}

/// Memory (MB) the instance's game gets, when set for this instance instead of Prism's default.
pub fn memory(instance: &Path) -> Option<u32> {
    let cfg = fs::read_to_string(instance.join("instance.cfg")).ok()?;
    if ini_get(&cfg, "OverrideMemory")? != "true" {
        return None;
    }
    ini_get(&cfg, "MaxMemAlloc")?.parse().ok()
}

/// Sets the instance's memory (MB, at least 512), or back to Prism's default with None. Prism
/// reads it on the next launch; while the instance runs, Prism would save over it.
pub fn set_memory(instance: &Path, mb: Option<u32>) -> Result<()> {
    anyhow::ensure!(mb.is_none_or(|mb| mb >= 512), "Minecraft needs at least 512 MB");
    let path = instance.join("instance.cfg");
    let cfg = fs::read_to_string(&path)?;
    let cfg = ini_set(&cfg, "OverrideMemory", if mb.is_some() { "true" } else { "false" });
    let cfg = match mb {
        Some(mb) => {
            ini_set(&ini_set(&cfg, "MaxMemAlloc", &mb.to_string()), "MinMemAlloc", &(mb / 4).max(512).min(mb).to_string())
        }
        None => cfg,
    };
    crate::store::write_atomic(&path, cfg.as_bytes())
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

/// `instance.cfg` keys that only make sense on the machine that wrote them.
const MACHINE_KEYS: &[&str] = &[
    "JavaPath",
    "OverrideJavaLocation",
    "JavaSignature",
    "JavaVersion",
    "JavaArchitecture",
    "JavaRealArchitecture",
    "JavaVendor",
    "shortcuts",
    "uuid",
];

/// Drops machine specific settings (Java path, shortcuts, ids) from an instance copied from
/// another computer, so Prism picks a local Java on first launch.
pub fn make_portable(instance: &Path) -> Result<()> {
    let path = instance.join("instance.cfg");
    let text = fs::read_to_string(&path)?;
    let kept: Vec<&str> =
        text.lines().filter(|l| l.split_once('=').is_none_or(|(k, _)| !MACHINE_KEYS.contains(&k.trim()))).collect();
    fs::write(path, kept.join("\n") + "\n")?;
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
    // Skips Minecraft's first-run accessibility screen, which would hold the game on the title
    // screen instead of joining the pack's server. options.txt is never versioned.
    let options = dir.join("minecraft/options.txt");
    if !options.exists() {
        fs::write(options, "onboardAccessibility:false\n")?;
    }
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

/// Prism's built-in icons, pinned to a commit so paths never move. Fetched on first use and
/// cached instead of bundled: Prism's art is GPL / CC BY-SA, BeaconSync is MIT.
const PRISM_ICONS: &str = "https://raw.githubusercontent.com/PrismLauncher/PrismLauncher/059e858e101edc9314ccb94aa48bd01b282539c5/launcher/resources/multimc";

const IMAGE_TYPES: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("svg", "image/svg+xml"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("ico", "image/x-icon"),
];

/// Prism's icons folder, honoring a custom `IconsDir` in `prismlauncher.cfg`.
fn prism_icons_dir() -> Option<PathBuf> {
    let data = prism_data_dir()?;
    let cfg = fs::read_to_string(data.join("prismlauncher.cfg")).unwrap_or_default();
    Some(match ini_get(&cfg, "IconsDir").filter(|v| !v.is_empty()) {
        Some(dir) if Path::new(&dir).is_absolute() => PathBuf::from(dir),
        Some(dir) => data.join(dir),
        None => data.join("icons"),
    })
}

/// `<dir>/<key>.<image ext>` if present, with its mime type.
fn find_image(dir: &Path, key: &str) -> Option<(PathBuf, &'static str)> {
    IMAGE_TYPES.iter().map(|(ext, mime)| (dir.join(format!("{key}.{ext}")), *mime)).find(|(p, _)| p.is_file())
}

/// The instance's Prism icon as (bytes, mime type): a custom one from Prism's icons folder,
/// else Prism's built-in icon of that name (downloaded once, then cached). None when the
/// key is unknown or the download fails; callers show a placeholder.
pub async fn icon(instance: &Path) -> Option<(Vec<u8>, &'static str)> {
    let cfg = fs::read_to_string(instance.join("instance.cfg")).ok()?;
    let key = ini_get(&cfg, "iconKey").filter(|k| !k.is_empty()).unwrap_or_else(|| "default".into());
    let key = if key == "default" { "grass".to_string() } else { key };
    if !key.chars().all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c)) || key.starts_with('.') {
        return None;
    }
    let cache = data_dir().join("icons");
    if let Some((path, mime)) = prism_icons_dir().and_then(|d| find_image(&d, &key)).or_else(|| find_image(&cache, &key)) {
        return Some((fs::read(path).ok()?, mime));
    }
    let missing = cache.join(format!("{key}.missing"));
    if missing.exists() {
        return None;
    }
    for (size, ext, mime) in [
        ("scalable", "svg", "image/svg+xml"),
        ("128x128", "png", "image/png"),
        ("50x50", "png", "image/png"),
        ("32x32", "png", "image/png"),
    ] {
        let resp = crate::http().get(format!("{PRISM_ICONS}/{size}/instances/{key}.{ext}")).send().await.ok()?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            continue;
        }
        let bytes = resp.error_for_status().ok()?.bytes().await.ok()?.to_vec();
        fs::create_dir_all(&cache).ok()?;
        fs::write(cache.join(format!("{key}.{ext}")), &bytes).ok()?;
        return Some((bytes, mime));
    }
    // Not a Prism icon (e.g. a custom icon that was deleted); don't ask again.
    fs::create_dir_all(&cache).ok()?;
    fs::write(missing, b"").ok()?;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_set_replaces_in_place_or_adds_under_general() {
        let qt = "[General]\nConfigVersion=1.3\nMaxMemAlloc=4096\n[UI]\nx=1\n";
        assert_eq!(ini_set(qt, "MaxMemAlloc", "8192"), "[General]\nConfigVersion=1.3\nMaxMemAlloc=8192\n[UI]\nx=1\n");
        assert_eq!(
            ini_set(qt, "OverrideMemory", "true"),
            "[General]\nOverrideMemory=true\nConfigVersion=1.3\nMaxMemAlloc=4096\n[UI]\nx=1\n"
        );
        assert_eq!(ini_set("name=Pack\n", "MaxMemAlloc", "2048"), "name=Pack\nMaxMemAlloc=2048\n");
    }

    #[test]
    fn memory_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("instance.cfg"), "name=Pack\n").unwrap();
        assert_eq!(memory(dir.path()), None);
        set_memory(dir.path(), Some(6144)).unwrap();
        assert_eq!(memory(dir.path()), Some(6144));
        set_memory(dir.path(), None).unwrap();
        assert_eq!(memory(dir.path()), None);
        assert!(set_memory(dir.path(), Some(256)).is_err());
    }
}
