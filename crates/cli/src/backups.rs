//! Server backups: BeaconSync's own whole server archives in `.beacon/backups`, plus the world
//! archives backup mods (FTB Backups, Simple Backups...) leave in top level `*backup*` folders.
//!
//! Ids are paths relative to the server folder, and only ids from [`list`] are acted on.

use anyhow::{Context, Result, bail};
use beacon_core::{api::Backup, server as srv};
use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

/// BeaconSync backups kept; older ones are pruned after each new one.
const KEEP: usize = 10;

/// Backup mods by a piece of their jar name, for labels.
const MODS: &[(&str, &str)] = &[
    ("ftbbackups", "FTB Backups"),
    ("ftb-backups", "FTB Backups"),
    ("simplebackups", "Simple Backups"),
    ("simple-backups", "Simple Backups"),
    ("simple_backups", "Simple Backups"),
    ("textile_backup", "Textile Backup"),
    ("advancedbackups", "Advanced Backups"),
    ("justenoughbackups", "Just Enough Backups"),
    ("chronos", "Chronos Backups"),
];

fn own_dir(server: &Path) -> PathBuf {
    server.join(".beacon/backups")
}

fn is_archive(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".zip") || n.ends_with(".tar.gz") || n.ends_with(".tgz")
}

/// Top level folders with "backup" in their name: where backup mods write.
fn mod_dirs(server: &Path) -> Vec<PathBuf> {
    fs::read_dir(server)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_type().is_ok_and(|t| t.is_dir()) && e.file_name().to_string_lossy().to_ascii_lowercase().contains("backup")
        })
        .map(|e| e.path())
        .collect()
}

/// The installed backup mod's name, else the folder name.
fn mod_label(server: &Path, folder: &Path) -> String {
    let jars: Vec<String> = fs::read_dir(server.join("mods"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_ascii_lowercase())
        .collect();
    MODS.iter()
        .find(|(key, _)| jars.iter().any(|j| j.contains(key)))
        .map(|(_, name)| name.to_string())
        .unwrap_or_else(|| format!("{}/", folder.file_name().unwrap_or_default().to_string_lossy()))
}

fn mtime(p: &Path) -> i64 {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

fn rel(server: &Path, p: &Path) -> String {
    p.strip_prefix(server).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

/// Moves the single backup older versions kept into the backups folder.
fn migrate(server: &Path) {
    let old = server.join(".beacon/backup.tar.gz");
    if old.is_file() && fs::create_dir_all(own_dir(server)).is_ok() {
        fs::rename(&old, own_dir(server).join(format!("{}-before-update.tar.gz", mtime(&old)))).ok();
    }
}

/// Every backup on the server, newest first.
pub fn list(server: &Path) -> Vec<Backup> {
    migrate(server);
    let mut out = vec![];
    for e in fs::read_dir(own_dir(server)).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".tar.gz") else { continue };
        let note = stem.split_once('-').map_or("", |(_, n)| n).replace('-', " ");
        let size = e.metadata().map_or(0, |m| m.len());
        out.push(Backup {
            id: rel(server, &e.path()),
            source: "BeaconSync".into(),
            note,
            size,
            created: mtime(&e.path()),
            full: true,
        });
    }
    for folder in mod_dirs(server) {
        let source = mod_label(server, &folder);
        for e in walkdir::WalkDir::new(&folder).max_depth(3).into_iter().flatten() {
            let name = e.file_name().to_string_lossy();
            if !e.file_type().is_file() || !is_archive(&name) {
                continue;
            }
            out.push(Backup {
                id: rel(server, e.path()),
                source: source.clone(),
                // Mods often nest archives by world and date, e.g. world/2026-11-05_15-25-37/full.zip.
                note: rel(&folder, e.path()),
                size: e.metadata().map_or(0, |m| m.len()),
                created: mtime(e.path()),
                full: false,
            });
        }
    }
    out.sort_by_key(|b| std::cmp::Reverse(b.created));
    out
}

fn find(server: &Path, id: &str) -> Result<Backup> {
    list(server).into_iter().find(|b| b.id == id).with_context(|| format!("no backup {id}"))
}

/// Left out of BeaconSync backups: caches, logs, other backups and files being written.
/// `world` is the world folder, always kept even if its name contains "backup".
fn skip(server: &Path, world: &str, rel: &Path) -> bool {
    let s = rel.to_string_lossy();
    let top = rel.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
    s.starts_with(".beacon/backup")
        || s.starts_with(".beacon/objects")
        || s.starts_with(".beacon/bin")
        || s.starts_with(".beacon/home")
        || s.starts_with(".beacon/console")
        || s.starts_with(".beacon/run.pid")
        || s.starts_with(".fabric")
        || s.starts_with("logs")
        || s.starts_with("crash-reports")
        || s.ends_with(".part")
        || (top != world && top.to_ascii_lowercase().contains("backup") && server.join(&top).is_dir())
}

/// Archives the whole server (worlds included) as a new BeaconSync backup. Call [`prune`] after.
pub fn create(server: &Path, note: &str) -> Result<Backup> {
    let dir = own_dir(server);
    fs::create_dir_all(&dir)?;
    let slug: String = note.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '-' }).collect();
    let name = format!("{}-{}.tar.gz", beacon_core::now(), slug.trim_matches('-'));
    let tmp = dir.join(format!("{name}.part"));
    {
        let enc = flate2::write::GzEncoder::new(fs::File::create(&tmp)?, flate2::Compression::fast());
        let mut tar = tar::Builder::new(enc);
        tar.follow_symlinks(false);
        let world = level_name(server);
        let walk = walkdir::WalkDir::new(server).min_depth(1).into_iter();
        for e in walk.filter_entry(|e| !skip(server, &world, e.path().strip_prefix(server).unwrap_or(e.path()))) {
            let e = e?;
            if e.file_type().is_file() {
                tar.append_path_with_name(e.path(), e.path().strip_prefix(server)?)?;
            }
        }
        tar.into_inner()?.finish()?;
    }
    fs::rename(&tmp, dir.join(&name))?;
    find(server, &rel(server, &dir.join(name)))
}

/// Keeps the newest BeaconSync backups. Separate from [`create`] so a restore can take a safety
/// backup without pruning the one it is about to restore.
pub fn prune(server: &Path) {
    let mut own: Vec<Backup> = list(server).into_iter().filter(|b| b.full).collect();
    for old in own.split_off(KEEP.min(own.len())) {
        fs::remove_file(server.join(&old.id)).ok();
    }
}

/// The newest BeaconSync backup, e.g. the one taken before the last update.
pub fn latest(server: &Path) -> Option<Backup> {
    list(server).into_iter().find(|b| b.full)
}

pub fn delete(server: &Path, id: &str) -> Result<()> {
    let b = find(server, id)?;
    fs::remove_file(server.join(&b.id))?;
    Ok(())
}

/// The world folder name from `server.properties`.
fn level_name(server: &Path) -> String {
    fs::read_to_string(server.join("server.properties"))
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("level-name=").map(|v| v.trim().to_string())))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "world".into())
}

/// Restores a backup. Minecraft must be stopped. Whole server backups replace the pack files
/// and worlds; mod backups replace only the world they contain.
pub fn restore(server: &Path, id: &str) -> Result<()> {
    let b = find(server, id)?;
    let file = server.join(&b.id);
    if b.full {
        if let Some(tree) = srv::applied_tree(server) {
            for p in tree.files.keys() {
                fs::remove_file(server.join(p)).ok();
            }
        }
        // Chunks generated after the backup would otherwise survive the restore.
        fs::remove_dir_all(server.join(level_name(server))).ok();
        tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(file)?)).unpack(server)?;
        return Ok(());
    }
    let name = b.id.to_ascii_lowercase();
    if name.contains("incremental") || name.contains("differential") {
        bail!("{} only holds changes since an earlier backup; restore it from {} itself", b.note, b.source);
    }
    let entries = archive_entries(&file)?;
    // Either the world's files at the root, or one folder holding them.
    let (dest, clear) = if entries.iter().any(|e| e == "level.dat") {
        let world = server.join(level_name(server));
        (world.clone(), world)
    } else if let Some(top) = entries.iter().find_map(|e| e.strip_suffix("/level.dat").filter(|t| !t.contains('/'))) {
        (server.to_path_buf(), server.join(top))
    } else {
        bail!("{} doesn't look like a world backup (no level.dat)", b.note);
    };
    fs::remove_dir_all(&clear).ok();
    fs::create_dir_all(&dest)?;
    if b.id.to_ascii_lowercase().ends_with(".zip") {
        zip::ZipArchive::new(fs::File::open(&file)?)?.extract(&dest)?;
    } else {
        tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(&file)?)).unpack(&dest)?;
    }
    Ok(())
}

fn archive_entries(file: &Path) -> Result<Vec<String>> {
    if file.to_string_lossy().to_ascii_lowercase().ends_with(".zip") {
        let z = zip::ZipArchive::new(fs::File::open(file)?)?;
        return Ok(z.file_names().map(|n| n.trim_start_matches("./").to_string()).collect());
    }
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(file)?));
    let mut out = vec![];
    for e in tar.entries()? {
        out.push(e?.path()?.to_string_lossy().trim_start_matches("./").to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn server() -> PathBuf {
        let d = std::env::temp_dir().join(format!("beacon-backups-{}-{}", std::process::id(), rand_suffix()));
        fs::create_dir_all(d.join("world/region")).unwrap();
        fs::write(d.join("world/level.dat"), "old").unwrap();
        fs::write(d.join("server.properties"), "level-name=world\n").unwrap();
        d
    }

    /// Unique per test: the clock alone repeats across parallel tests on macOS (microseconds).
    fn rand_suffix() -> u64 {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    #[test]
    fn full_backup_round_trips_and_leaves_newer_chunks_out() {
        let d = server();
        let b = create(&d, "before 1.4").unwrap();
        assert_eq!((b.source.as_str(), b.note.as_str(), b.full), ("BeaconSync", "before 1.4", true));
        fs::write(d.join("world/level.dat"), "new").unwrap();
        fs::write(d.join("world/region/r.0.0.mca"), "chunk").unwrap();
        restore(&d, &b.id).unwrap();
        assert_eq!(fs::read_to_string(d.join("world/level.dat")).unwrap(), "old");
        assert!(!d.join("world/region/r.0.0.mca").exists());
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn mod_zip_backups_are_listed_and_restore_the_world() {
        let d = server();
        fs::create_dir_all(d.join("backups")).unwrap();
        fs::create_dir_all(d.join("mods")).unwrap();
        fs::write(d.join("mods/ftbbackups2-fabric-1.0.jar"), "").unwrap();
        let mut z = zip::ZipWriter::new(fs::File::create(d.join("backups/2026-11-05.zip")).unwrap());
        z.start_file("world/level.dat", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(b"from mod").unwrap();
        z.finish().unwrap();
        let b = list(&d).into_iter().find(|b| !b.full).unwrap();
        assert_eq!(
            (b.id.as_str(), b.source.as_str(), b.note.as_str()),
            ("backups/2026-11-05.zip", "FTB Backups", "2026-11-05.zip")
        );
        // Our own backups leave mod backups out.
        let own = create(&d, "manual").unwrap();
        assert!(!archive_entries(&d.join(own.id)).unwrap().iter().any(|e| e.starts_with("backups/")));
        restore(&d, &b.id).unwrap();
        assert_eq!(fs::read_to_string(d.join("world/level.dat")).unwrap(), "from mod");
        assert!(restore(&d, "../etc/passwd").is_err());
        // Partial archives from incremental backups are refused rather than half restored.
        fs::copy(d.join("backups/2026-11-05.zip"), d.join("backups/incremental.zip")).unwrap();
        assert!(restore(&d, "backups/incremental.zip").is_err());
        fs::remove_dir_all(d).ok();
    }
}
