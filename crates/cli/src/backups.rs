//! Server backups: BeaconSync's own whole server archives in `.beacon/backups`, plus the world
//! archives backup mods (FTB Backups, Simple Backups...) leave in top level `*backup*` folders.
//!
//! Ids are paths relative to the server folder, and only ids from [`list`] are acted on.

use anyhow::{Context, Result, bail};
use beacon_core::{
    api::{Backup, Retention},
    server as srv,
};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

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

/// Kept apart from `server.json` and left out of backups (see [`skip`]), so restoring an older
/// backup doesn't bring back an older rule.
fn retention_path(server: &Path) -> PathBuf {
    server.join(".beacon/backup-retention.json")
}

/// The server's retention rule; the default until the owner sets one.
pub fn retention(server: &Path) -> Retention {
    fs::read(retention_path(server)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Saves the rule and prunes by it right away.
pub fn set_retention(server: &Path, r: &Retention) -> Result<()> {
    beacon_core::store::write_atomic(&retention_path(server), &serde_json::to_vec_pretty(r)?)?;
    prune(server);
    Ok(())
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

/// Left out of BeaconSync backups: caches, logs, other backups (and their retention rule) and
/// files being written.
/// `world` is the world's top folder, always kept even if its name contains "backup".
fn skip(server: &Path, world: &str, rel: &Path) -> bool {
    // Forward slashes on every OS, so the prefixes below also match Windows paths.
    let s = rel.to_string_lossy().replace('\\', "/");
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
    // Unique per call, so concurrent backups never share (or delete) each other's temp file.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = dir.join(format!("{name}.{}-{seq}.part", std::process::id()));
    {
        // Owner-only on Unix: the archive holds `.beacon/server.json` and its hub token. Created
        // exclusively, so nothing already at that path is ever reused.
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let enc = flate2::write::GzEncoder::new(options.open(&tmp)?, flate2::Compression::fast());
        let written = (|| -> Result<()> {
            let mut tar = tar::Builder::new(enc);
            tar.follow_symlinks(false);
            // The world's top folder, normalized like restore's `world_dir` (`world/` is `world`),
            // so the world restore clears is always one the backup kept, even when its name
            // contains "backup".
            let world = level_name(server);
            let world = world_rel(&world)
                .and_then(|w| w.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()))
                .unwrap_or(world);
            let walk = walkdir::WalkDir::new(server).min_depth(1).into_iter();
            for e in walk.filter_entry(|e| !skip(server, &world, e.path().strip_prefix(server).unwrap_or(e.path()))) {
                let e = e?;
                if e.file_type().is_file() {
                    tar.append_path_with_name(e.path(), e.path().strip_prefix(server)?)?;
                }
            }
            tar.into_inner()?.finish()?;
            fs::rename(&tmp, dir.join(&name))?;
            Ok(())
        })();
        // A failed backup (disk full, say) must not leave a partial archive outside retention.
        if written.is_err() {
            fs::remove_file(&tmp).ok();
        }
        written?;
    }
    find(server, &rel(server, &dir.join(name)))
}

/// Deletes the BeaconSync backups past the server's [`retention`]. Separate from [`create`] so a
/// restore can take a safety backup without pruning the one it is about to restore.
pub fn prune(server: &Path) {
    let own: Vec<Backup> = list(server).into_iter().filter(|b| b.full).collect();
    for old in &own[kept(&own, &retention(server), beacon_core::now())..] {
        fs::remove_file(server.join(&old.id)).ok();
    }
}

/// How many of `own` (newest first) `r` keeps. Each limit only grows more exceeded going back
/// in time, so everything from the first backup past one is dropped. The newest always stays.
fn kept(own: &[Backup], r: &Retention, now: i64) -> usize {
    let mut total = 0;
    own.iter()
        .enumerate()
        .position(|(i, b)| {
            total += b.size;
            i > 0
                && (r.keep.is_some_and(|k| i >= k as usize)
                    || r.max_bytes.is_some_and(|m| total > m)
                    || r.max_days.is_some_and(|d| now - b.created > i64::from(d) * 86_400))
        })
        .unwrap_or(own.len())
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

/// `name` as one plain folder name, so a restore only ever clears a folder directly inside the
/// server (not `..`, `.`, an absolute path or a nested one). Callers join the returned part, not
/// `name`, so a trailing `/` can't make deletion follow a symlinked folder.
fn one_folder(name: &str) -> Option<&std::ffi::OsStr> {
    let mut parts = Path::new(name).components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(part)), None) => Some(part),
        _ => None,
    }
}

/// `level-name` as a relative path of plain folder names (`world`, `worlds/survival`), rebuilt
/// from its parts so `world/` is `world`. `None` for `..`, `.`, a root or a drive: the world must
/// sit inside the server.
fn world_rel(name: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in Path::new(name).components() {
        match part {
            Component::Normal(p) => out.push(p),
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// The world folder a restore clears, refused unless it sits inside the server.
fn world_dir(server: &Path) -> Result<PathBuf> {
    let name = level_name(server);
    let rel =
        world_rel(&name).with_context(|| format!("level-name {name:?} in server.properties isn't a folder inside the server"))?;
    Ok(server.join(rel))
}

/// The archive path, without `./` parts, when it names `.beacon/server.json` (the server's config,
/// hub token included) in any letter case: on a case-insensitive disk every spelling is the live
/// token. Callers write it under this spelling, so case-distinct files on other disks stay separate
/// files, each owner-only.
fn server_config_entry(path: &Path) -> Option<PathBuf> {
    let parts: Vec<_> = path.components().filter(|c| !matches!(c, Component::CurDir)).collect();
    match parts.as_slice() {
        [Component::Normal(dir), Component::Normal(file)]
            if dir.eq_ignore_ascii_case(".beacon") && file.eq_ignore_ascii_case("server.json") =>
        {
            Some(Path::new(dir).join(file))
        }
        _ => None,
    }
}

/// Removes a world folder before a restore. Missing is fine; anything else stops the restore.
fn clear(dir: &Path) -> Result<()> {
    match fs::remove_dir_all(dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e).with_context(|| format!("clearing {}", dir.display())),
        _ => Ok(()),
    }
}

/// A restore worked out by [`plan`], every check passed, before anything is deleted.
pub struct Plan {
    id: String,
    /// Opened before the checks and extracted from in [`apply`], so the archive that was checked
    /// is the one restored even if a backup mod deletes or replaces the file in between.
    archive: fs::File,
    kind: PlanKind,
}

enum PlanKind {
    /// A whole server backup: `dest` is the canonical server folder.
    Full { dest: PathBuf, world: PathBuf },
    /// A mod-made world backup, unpacked into `dest` after clearing `old`.
    World { zip: bool, dest: PathBuf, old: PathBuf },
}

impl Plan {
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Restores a backup: [`plan`], then [`apply`]. Minecraft must be stopped. Whole server backups
/// replace the pack files and worlds; mod backups replace only the world they contain.
pub fn restore(server: &Path, id: &str) -> Result<()> {
    apply(server, plan(server, id)?)
}

/// The server token's folder, refused if it is a link: the token is only ever written into a real
/// folder inside the server.
fn refuse_link(dest: &Path, token: &Path) -> Result<()> {
    let dir = dest.join(token.parent().unwrap_or(token));
    if fs::symlink_metadata(&dir).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("{} is a link; not restoring the server token through it", dir.display());
    }
    Ok(())
}

/// Every check a restore makes, without changing anything: safe to run while Minecraft is up, so
/// a bad backup is refused before the server is stopped or a safety backup is taken.
pub fn plan(server: &Path, id: &str) -> Result<Plan> {
    let b = find(server, id)?;
    let file = server.join(&b.id);
    let archive = fs::File::open(&file)?;
    if b.full {
        // Canonical like `Archive::unpack`, which also gives Windows its long path prefix.
        let dest = server.canonicalize()?;
        let world = world_dir(server)?;
        let tokens = check_full_archive(archive.try_clone()?)?;
        refuse_link(&dest, Path::new(".beacon/server.json"))?;
        for t in &tokens {
            refuse_link(&dest, t)?;
        }
        return Ok(Plan { id: b.id, archive, kind: PlanKind::Full { dest, world } });
    }
    let name = b.id.to_ascii_lowercase();
    if name.contains("incremental") || name.contains("differential") {
        bail!("{} only holds changes since an earlier backup; restore it from {} itself", b.note, b.source);
    }
    let zip = file.to_string_lossy().to_ascii_lowercase().ends_with(".zip");
    // Read through the kept handle, so the layout checked is the layout extracted.
    let entries = archive_entries(archive.try_clone()?, zip)?;
    // Either the world's files at the root, or one folder holding them.
    let (dest, old) = if entries.iter().any(|e| e == "level.dat") {
        let world = world_dir(server)?;
        (world.clone(), world)
    } else if let Some(top) = entries.iter().find_map(|e| e.strip_suffix("/level.dat").and_then(one_folder)) {
        (server.to_path_buf(), server.join(top))
    } else {
        bail!("{} doesn't look like a world backup (no level.dat)", b.note);
    };
    Ok(Plan { id: b.id, archive, kind: PlanKind::World { zip, dest, old } })
}

/// Carries out a [`plan`]. Minecraft must be stopped.
pub fn apply(server: &Path, plan: Plan) -> Result<()> {
    use std::io::{Seek, SeekFrom};
    let mut archive = plan.archive;
    // The checks read through a clone of this handle, which shares its position.
    archive.seek(SeekFrom::Start(0))?;
    let (zip, dest, old) = match plan.kind {
        PlanKind::World { zip, dest, old } => (zip, dest, old),
        PlanKind::Full { dest, world } => {
            // Chunks generated after the backup would otherwise survive, so a world that can't be
            // cleared stops the restore.
            clear(&world)?;
            if let Some(tree) = srv::applied_tree(server) {
                for p in tree.files.keys() {
                    fs::remove_file(server.join(p)).ok();
                }
            }
            let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(archive));
            for entry in archive.entries()? {
                let mut entry = entry?;
                // The server token goes back owner-only from the start, never through a default
                // mode, and only into the server's own `.beacon` folder.
                if let Some(rel) = server_config_entry(&entry.path()?) {
                    // Checked again right before writing, in case the archive created the link.
                    refuse_link(&dest, &rel)?;
                    let mut bytes = vec![];
                    entry.read_to_end(&mut bytes)?;
                    beacon_core::store::write_private(&dest.join(rel), &bytes)?;
                } else {
                    entry.unpack_in(&dest)?;
                }
            }
            return Ok(());
        }
    };
    clear(&old)?;
    fs::create_dir_all(&dest)?;
    if zip {
        zip::ZipArchive::new(archive)?.extract(&dest)?;
    } else {
        tar::Archive::new(flate2::read::GzDecoder::new(archive)).unpack(&dest)?;
    }
    Ok(())
}

/// Reads a whole server archive before a restore touches anything, returning the spellings of its
/// token entries. Refuses a truncated or corrupted backup (every entry, then the rest of the gzip
/// stream so its checksum is checked: tar stops at its end marker) and a link entry where the
/// token's folder would be.
fn check_full_archive(file: fs::File) -> Result<Vec<PathBuf>> {
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let mut tokens = vec![];
    for entry in tar.entries()? {
        let entry = entry?;
        let path = entry.path()?;
        // BeaconSync writes relative paths only; tar would strip a root or drive and land these
        // somewhere the checks below don't look.
        if path.components().any(|c| matches!(c, Component::RootDir | Component::Prefix(_) | Component::ParentDir)) {
            bail!("the backup has an unexpected path {}", path.display());
        }
        let kind = entry.header().entry_type();
        if (kind.is_symlink() || kind.is_hard_link()) && server_config_entry(&path.join("server.json")).is_some() {
            bail!("the backup links {}; not restoring the server token through it", path.display());
        }
        tokens.extend(server_config_entry(&path));
    }
    std::io::copy(&mut tar.into_inner(), &mut std::io::sink()).context("the backup is damaged")?;
    Ok(tokens)
}

fn archive_entries(file: fs::File, zip: bool) -> Result<Vec<String>> {
    if zip {
        let z = zip::ZipArchive::new(file)?;
        return Ok(z
            .file_names()
            .map(|n| n.map(|n| n.trim_start_matches("./").to_string()))
            .collect::<zip::result::ZipResult<_>>()?);
    }
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let mut out = vec![];
    for e in tar.entries()? {
        out.push(e?.path()?.to_string_lossy().trim_start_matches("./").to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(tar_gz: &Path) -> Vec<String> {
        archive_entries(fs::File::open(tar_gz).unwrap(), false).unwrap()
    }

    const SERVER_CONFIG: &str = ".beacon/server.json";
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
        fs::create_dir_all(d.join(".beacon")).unwrap();
        fs::write(d.join(SERVER_CONFIG), "{\"token\":\"secret\"}").unwrap();
        let b = create(&d, "before 1.4").unwrap();
        assert_eq!((b.source.as_str(), b.note.as_str(), b.full), ("BeaconSync", "before 1.4", true));
        fs::write(d.join("world/level.dat"), "new").unwrap();
        fs::write(d.join("world/region/r.0.0.mca"), "chunk").unwrap();
        restore(&d, &b.id).unwrap();
        assert_eq!(fs::read_to_string(d.join("world/level.dat")).unwrap(), "old");
        assert!(!d.join("world/region/r.0.0.mca").exists());
        // The archive and the restored token are owner-only.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&d.join(&b.id)), 0o600);
            assert_eq!(mode(&d.join(SERVER_CONFIG)), 0o600);
        }
        assert_eq!(fs::read_to_string(d.join(SERVER_CONFIG)).unwrap(), "{\"token\":\"secret\"}");
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn a_world_named_like_a_backup_folder_is_still_backed_up() {
        let d = server();
        fs::create_dir_all(d.join("backup-world")).unwrap();
        fs::write(d.join("backup-world/level.dat"), "mine").unwrap();
        fs::write(d.join("server.properties"), "level-name=backup-world/\n").unwrap();
        let b = create(&d, "x").unwrap();
        assert!(entries(&d.join(&b.id)).iter().any(|e| e == "backup-world/level.dat"));
        fs::write(d.join("backup-world/level.dat"), "newer").unwrap();
        restore(&d, &b.id).unwrap();
        assert_eq!(fs::read_to_string(d.join("backup-world/level.dat")).unwrap(), "mine");
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn broken_or_linking_full_backups_are_refused() {
        let d = server();
        let good = create(&d, "good").unwrap();
        let own = own_dir(&d);
        // Truncated: refused before the world is touched.
        let bytes = fs::read(d.join(&good.id)).unwrap();
        fs::write(own.join("1-truncated.tar.gz"), &bytes[..bytes.len() / 2]).unwrap();
        assert!(find(&d, ".beacon/backups/1-truncated.tar.gz").unwrap().full);
        assert!(plan(&d, ".beacon/backups/1-truncated.tar.gz").is_err(), "refused by the checks alone");
        // Only the gzip footer missing, or one flipped byte: tar alone would read these fine.
        fs::write(own.join("1-footerless.tar.gz"), &bytes[..bytes.len() - 8]).unwrap();
        assert!(restore(&d, ".beacon/backups/1-footerless.tar.gz").is_err());
        let mut flipped = bytes.clone();
        let len = flipped.len();
        flipped[len - 5] ^= 0xff;
        fs::write(own.join("1-flipped.tar.gz"), &flipped).unwrap();
        assert!(restore(&d, ".beacon/backups/1-flipped.tar.gz").is_err());
        assert_eq!(fs::read_to_string(d.join("world/level.dat")).unwrap(), "old");
        // A `.Beacon` link made by the archive itself is never written through.
        #[cfg(unix)]
        {
            let outside = std::env::temp_dir().join(format!("beacon-outside-{}", rand_suffix()));
            fs::create_dir_all(&outside).unwrap();
            let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
                fs::File::create(own.join("2-linking.tar.gz")).unwrap(),
                flate2::Compression::fast(),
            ));
            let mut link = tar::Header::new_gnu();
            link.set_entry_type(tar::EntryType::Symlink);
            link.set_size(0);
            tar.append_link(&mut link, ".Beacon", &outside).unwrap();
            let mut file = tar::Header::new_gnu();
            file.set_size(2);
            tar.append_data(&mut file, ".Beacon/server.json", &b"{}"[..]).unwrap();
            tar.into_inner().unwrap().finish().unwrap();
            assert!(find(&d, ".beacon/backups/2-linking.tar.gz").unwrap().full);
            let err = restore(&d, ".beacon/backups/2-linking.tar.gz").unwrap_err().to_string();
            assert!(err.contains("links"), "{err}");
            assert!(!outside.join("server.json").exists());
            assert_eq!(fs::read_to_string(d.join("world/level.dat")).unwrap(), "old", "refused before deleting");
            // The same link spelled with a root (`/.beacon`), which tar would strip on unpack.
            let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
                fs::File::create(own.join("3-rooted.tar.gz")).unwrap(),
                flate2::Compression::fast(),
            ));
            let mut link = tar::Header::new_gnu();
            link.as_gnu_mut().unwrap().name[..8].copy_from_slice(b"/.beacon");
            link.set_entry_type(tar::EntryType::Symlink);
            link.set_link_name(&outside).unwrap();
            link.set_size(0);
            link.set_cksum();
            tar.append(&link, std::io::empty()).unwrap();
            tar.into_inner().unwrap().finish().unwrap();
            let err = plan(&d, ".beacon/backups/3-rooted.tar.gz").err().unwrap().to_string();
            assert!(err.contains("unexpected path"), "{err}");
            assert_eq!(fs::read_to_string(d.join("world/level.dat")).unwrap(), "old");
            fs::remove_dir_all(outside).ok();
        }
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
        assert!(!entries(&d.join(&own.id)).iter().any(|e| e.starts_with("backups/")));
        restore(&d, &b.id).unwrap();
        assert_eq!(fs::read_to_string(d.join("world/level.dat")).unwrap(), "from mod");
        assert!(restore(&d, "../etc/passwd").is_err());
        // An archive naming `..` as its world is refused before anything is cleared.
        let mut z = zip::ZipWriter::new(fs::File::create(d.join("backups/escape.zip")).unwrap());
        z.start_file("../level.dat", zip::write::SimpleFileOptions::default()).unwrap();
        z.finish().unwrap();
        assert!(restore(&d, "backups/escape.zip").is_err());
        assert!(d.join("world/level.dat").exists());
        // So is a server.properties pointing the world outside the server, and pack files survive.
        let mut tree = beacon_core::model::Tree::default();
        tree.files.insert("mods/pack.jar".into(), serde_json::from_value(serde_json::json!({ "hash": "x", "size": 0 })).unwrap());
        fs::write(d.join(".beacon/server-tree.json"), serde_json::to_vec(&tree).unwrap()).unwrap();
        fs::write(d.join("mods/pack.jar"), "").unwrap();
        fs::write(d.join("server.properties"), "level-name=../elsewhere\n").unwrap();
        assert!(restore(&d, &own.id).is_err());
        assert!(d.join("world/level.dat").exists() && d.join("mods/pack.jar").exists(), "a refused restore deletes nothing");
        assert_eq!(one_folder("world/"), Some(std::ffi::OsStr::new("world")));
        assert_eq!(world_rel("worlds/survival/"), Some(PathBuf::from("worlds/survival")));
        assert_eq!(world_rel("../x").or(world_rel("/abs")).or(world_rel(".")), None);
        assert_eq!(server_config_entry(Path::new("./.Beacon/Server.json")), Some(PathBuf::from(".Beacon/Server.json")));
        assert_eq!(server_config_entry(Path::new("x/.beacon/server.json")), None);
        fs::write(d.join("server.properties"), "level-name=world\n").unwrap();
        // Partial archives from incremental backups are refused rather than half restored.
        fs::copy(d.join("backups/2026-11-05.zip"), d.join("backups/incremental.zip")).unwrap();
        assert!(restore(&d, "backups/incremental.zip").is_err());
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn retention_drops_the_oldest_past_any_limit_but_keeps_the_newest() {
        const DAY: i64 = 86_400;
        let gb = 1 << 30;
        // Newest first: 1, 2, 3... days old, 1 GB each.
        let own: Vec<Backup> = (1..=6)
            .map(|d| Backup {
                id: d.to_string(),
                source: "BeaconSync".into(),
                note: String::new(),
                size: gb,
                created: -d * DAY,
                full: true,
            })
            .collect();
        let r = |keep, gbs: Option<u64>, max_days| Retention { keep, max_bytes: gbs.map(|g| g * gb), max_days };
        assert_eq!(kept(&own, &r(None, None, None), 0), 6);
        assert_eq!(kept(&own, &r(Some(5), None, None), 0), 5);
        assert_eq!(kept(&own, &r(None, Some(3), None), 0), 3);
        assert_eq!(kept(&own, &r(None, None, Some(4)), 0), 4);
        assert_eq!(kept(&own, &r(Some(5), Some(4), Some(2)), 0), 2);
        assert_eq!(kept(&own, &r(Some(0), Some(0), Some(0)), 0), 1);
    }

    #[test]
    fn retention_is_applied_and_survives_a_restore() {
        let d = server();
        create(&d, "one").unwrap();
        set_retention(&d, &Retention { keep: Some(5), max_bytes: None, max_days: None }).unwrap();
        create(&d, "two").unwrap();
        set_retention(&d, &Retention { keep: Some(1), max_bytes: None, max_days: None }).unwrap();
        assert_eq!(list(&d).len(), 1);
        // Restoring a backup taken under keep 5 leaves keep 1 in place.
        restore(&d, &list(&d)[0].id).unwrap();
        assert_eq!(retention(&d).keep, Some(1));
        fs::remove_dir_all(d).ok();
    }
}
