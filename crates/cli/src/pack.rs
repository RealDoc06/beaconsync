//! Git-like pack commands that operate on an instance folder.

use crate::ui::{self, Bar};
use anyhow::{Context, Result, bail};
use beacon_core::{
    api::{AllServers, DeployRequest, DeployTargets},
    diff::ChangeKind,
    fetch,
    hash::short,
    hub::{self, Hub},
    import, instance, meta,
    model::{Loader, LoaderKind},
    ops, prism,
    repo::{self, Repo, Role},
    server,
    settings::Settings,
    sources::modrinth,
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(clap::ValueEnum, Clone, Copy)]
pub enum ExportKind {
    Mrpack,
    Server,
}

pub(crate) fn author() -> String {
    Settings::load().username.or_else(|| std::env::var("USER").ok()).unwrap_or_else(|| "me".into())
}

/// Finds the repo containing `dir` (works from inside `minecraft/` too).
fn find(dir: &Path) -> Result<Repo> {
    let mut cur = Some(dir);
    while let Some(d) = cur {
        if Repo::is_repo(d) {
            return Repo::open(d);
        }
        cur = d.parent();
    }
    bail!("not inside a BeaconSync pack (run `beacon init` in an instance folder)")
}

/// Parses `fabric` or `fabric@0.16.9`; without a version, the newest stable one for `mc`.
pub(crate) async fn parse_loader(s: &str, mc: &str) -> Result<Loader> {
    let (kind, version) = s.split_once('@').map(|(k, v)| (k, Some(v.to_string()))).unwrap_or((s, None));
    let kind = match kind.to_lowercase().as_str() {
        "vanilla" | "none" => LoaderKind::Vanilla,
        "fabric" => LoaderKind::Fabric,
        "quilt" => LoaderKind::Quilt,
        "forge" => LoaderKind::Forge,
        "neoforge" => LoaderKind::Neoforge,
        other => bail!("unknown loader {other}"),
    };
    let version = match (kind, version) {
        (LoaderKind::Vanilla, _) => None,
        (_, Some(v)) => Some(v),
        (k, None) => {
            let versions = meta::loader_versions(k, mc).await?;
            Some(
                versions
                    .iter()
                    .find(|v| v.stable)
                    .or(versions.first())
                    .context("no loader versions for that Minecraft version")?
                    .version
                    .clone(),
            )
        }
    };
    Ok(Loader { kind, version })
}

pub async fn new(name: &str, mc: &str, loader: &str) -> Result<()> {
    let loader = parse_loader(loader, mc).await?;
    let kind = loader.kind;
    let dir = instance::unique_dir(&Settings::load().instances_root(), name);
    instance::create(&dir, name, mc, &loader)?;
    let repo = Repo::init(&dir, repo::local_config(name))?;
    repo.commit("New pack", &author()).await?;
    ui::ok(format!("Created {name} ({} {mc}) at {}", kind.label(), dir.display()));
    Ok(())
}

pub async fn init(dir: &Path) -> Result<()> {
    if !dir.join("mmc-pack.json").is_file() {
        bail!("{} is not a Prism/MultiMC instance (no mmc-pack.json)", dir.display());
    }
    import::adopt(dir, &author()).await?;
    ui::ok("Tracking this instance. Saved the first version.");
    Ok(())
}

pub async fn import(file: Option<PathBuf>, modrinth: Option<String>) -> Result<()> {
    let s = Settings::load();
    let bar = Bar::new();
    let on = |p| bar.update(p);
    let root = s.instances_root();
    let repo = match (file, modrinth) {
        (_, Some(slug)) => import::install_modrinth_pack(&slug, None, &root, &author(), &on).await?,
        (Some(f), None) => import::import_file(&f, &root, s.curseforge_key.as_deref(), &author(), &on).await?,
        (None, None) => bail!("pass a file or --modrinth <slug>"),
    };
    bar.done();
    ui::ok(format!("Imported {} to {}", repo.config()?.name, repo.root.display()));
    Ok(())
}

pub async fn join(code: &str, fork: bool) -> Result<()> {
    let s = Settings::load();
    let (hub_url, code) = hub::parse_link(code).unwrap_or_else(|| (s.hub_url(), hub::parse_code(code)));
    let hub = Hub::new(&hub_url, s.token.clone());
    let bar = Bar::new();
    let repo = ops::join(&hub, &code, &s.instances_root(), fork, &author(), &|p| bar.update(p)).await?;
    bar.done();
    ui::ok(format!("{} is ready at {}", repo.config()?.name, repo.root.display()));
    Ok(())
}

fn kind_mark(k: ChangeKind) -> String {
    match k {
        ChangeKind::Added => console::style("+").green().to_string(),
        ChangeKind::Removed => console::style("-").red().to_string(),
        ChangeKind::Modified => console::style("~").yellow().to_string(),
        ChangeKind::Enabled => console::style("●").green().to_string(),
        ChangeKind::Disabled => console::style("○").dim().to_string(),
    }
}

pub fn status(dir: &Path) -> Result<()> {
    let repo = find(dir)?;
    let st = repo.status()?;
    let cfg = repo.config()?;
    let at = match (&st.branch, &st.head) {
        (Some(b), _) => format!("on {}", ui::bold(b)),
        (None, Some(h)) => format!("at {}", short(h)),
        _ => String::new(),
    };
    println!("{} {at}", ui::bold(&cfg.name));
    if st.merging {
        ui::warn("merge in progress: save to finish it");
    }
    if let Some((a, b)) = &st.diff.minecraft {
        println!("  {} Minecraft {a} -> {b}", kind_mark(ChangeKind::Modified));
    }
    if let Some((a, b)) = &st.diff.loader {
        println!(
            "  {} Loader {} {} -> {} {}",
            kind_mark(ChangeKind::Modified),
            a.kind.label(),
            a.version.as_deref().unwrap_or(""),
            b.kind.label(),
            b.version.as_deref().unwrap_or("")
        );
    }
    if st.diff.is_empty() {
        println!("{}", ui::dim("No unsaved changes"));
    }
    for c in &st.diff.files {
        let name = c.new.as_ref().or(c.old.as_ref()).and_then(|f| f.entry.name.clone());
        let label = match name {
            Some(n) => format!("{n} {}", ui::dim(&c.path)),
            None => c.path.clone(),
        };
        println!("  {} {label}", kind_mark(c.kind));
    }
    Ok(())
}

pub async fn save(dir: &Path, message: &str) -> Result<()> {
    let repo = find(dir)?;
    let id = repo.commit(message, &author()).await?;
    ui::ok(format!("Saved version {}", short(&id)));
    Ok(())
}

pub fn log(dir: &Path, limit: usize) -> Result<()> {
    let repo = find(dir)?;
    let Some(head) = repo.head_commit()? else {
        ui::info("No versions saved yet");
        return Ok(());
    };
    for e in repo.log(&head, limit)? {
        let marker = if e.id == head { "●" } else { "○" };
        println!(
            "{marker} {} {}  {}",
            ui::bold(short(&e.id)),
            e.commit.message,
            ui::dim(format!("{} · {}", e.commit.author, ui::ago(e.commit.time)))
        );
    }
    Ok(())
}

pub async fn discard(dir: &Path) -> Result<()> {
    let repo = find(dir)?;
    let bar = Bar::new();
    repo.discard(None, &|p| bar.update(p)).await?;
    bar.done();
    ui::ok("Unsaved changes discarded");
    Ok(())
}

pub async fn restore(dir: &Path, rev: &str) -> Result<()> {
    let repo = find(dir)?;
    let bar = Bar::new();
    repo.restore(rev, &|p| bar.update(p)).await?;
    bar.done();
    ui::ok(format!("Files restored from {rev}. Run `beacon save -m \"Restore {rev}\"` to keep it."));
    Ok(())
}

pub async fn rollback(dir: &Path, rev: &str, local: bool) -> Result<()> {
    let repo = find(dir)?;
    let hub = if local || repo.config()?.remote.is_none() { None } else { Some(hub_for(&repo)?) };
    let bar = Bar::new();
    let r = ops::rollback(&repo, hub.as_ref(), rev, &author(), &|p| bar.update(p)).await?;
    bar.done();
    match (r.release, hub) {
        (Some(rel), Some(hub)) => {
            ui::ok(format!("Rolled back. Published {} and deploying to {} server(s)", rel.name, r.servers));
            if let Some(remote) = repo.config()?.remote {
                follow_deploy(&hub, &remote.pack, &rel.commit).await?;
            }
        }
        _ => ui::ok(format!("Rolled back to {rev}, saved as {}", short(&r.commit))),
    }
    Ok(())
}

pub async fn reset(dir: &Path, rev: &str) -> Result<()> {
    let repo = find(dir)?;
    if !ui::confirm(&format!("Drop every version after {rev}?"), false)? {
        return Ok(());
    }
    let bar = Bar::new();
    repo.reset_hard(rev, &|p| bar.update(p)).await?;
    bar.done();
    ui::ok(format!("Back at {rev}"));
    Ok(())
}

pub fn branch(dir: &Path, name: Option<String>, delete: bool) -> Result<()> {
    let repo = find(dir)?;
    match (name, delete) {
        (Some(n), true) => {
            repo.delete_branch(&n)?;
            ui::ok(format!("Deleted branch {n}"));
        }
        (Some(n), false) => {
            repo.create_branch(&n, None)?;
            ui::ok(format!("Created branch {n}. Switch with `beacon switch {n}`."));
        }
        (None, _) => {
            for b in repo.branches()? {
                let mark = if b.current { "*" } else { " " };
                println!("{mark} {} {}", b.name, ui::dim(short(&b.commit)));
            }
        }
    }
    Ok(())
}

pub async fn switch(dir: &Path, target: &str, force: bool) -> Result<()> {
    let repo = find(dir)?;
    let bar = Bar::new();
    repo.switch(target, force, &|p| bar.update(p)).await?;
    bar.done();
    ui::ok(format!("Switched to {target}"));
    Ok(())
}

pub async fn merge(dir: &Path, branch: &str) -> Result<()> {
    let repo = find(dir)?;
    let bar = Bar::new();
    let r = repo.merge(branch, &|p| bar.update(p)).await?;
    bar.done();
    if r.fast_forward {
        ui::ok(format!("Fast-forwarded to {branch}"));
    } else if r.conflicts.is_empty() {
        ui::ok(format!("Merged {branch}. Save to finish: beacon save -m \"Merge {branch}\""));
    } else {
        ui::warn(format!("{} files changed on both sides; kept your version of:", r.conflicts.len()));
        for c in &r.conflicts {
            println!("  {}", c.ours.as_ref().or(c.theirs.as_ref()).map(|(p, _)| p.as_str()).unwrap_or(&c.id));
        }
        ui::info("Edit them if needed, then save. Use the app to pick sides per file.");
    }
    Ok(())
}

pub async fn add(dir: &Path, project: &str) -> Result<()> {
    let repo = find(dir)?;
    let tree = repo.scan()?;
    let installed: HashSet<String> = tree
        .files
        .values()
        .filter_map(|e| match &e.source {
            Some(beacon_core::model::Source::Modrinth { project, .. }) => Some(project.clone()),
            _ => None,
        })
        .collect();
    let files = modrinth::resolve_install(project, false, &tree.minecraft, tree.loader.kind, &installed).await?;
    let game = repo.game_dir();
    for (path, entry) in &files {
        fetch::download(entry.source.as_ref().map(|s| s.url()).unwrap_or_default(), &entry.hash, &game.join(path)).await?;
        ui::ok(format!("Added {} {}", entry.name.as_deref().unwrap_or(path), ui::dim(entry.version.as_deref().unwrap_or(""))));
    }
    repo.stage(files.into_iter().map(|(_, e)| e))?;
    Ok(())
}

pub async fn updates(dir: &Path, apply: bool) -> Result<()> {
    let repo = find(dir)?;
    let tree = repo.scan()?;
    let files: Vec<_> = tree.files.iter().map(|(p, e)| (p.clone(), e.clone())).collect();
    let ups = modrinth::check_updates(&files, &tree.minecraft, tree.loader.kind).await?;
    if ups.is_empty() {
        ui::ok("Everything is up to date");
        return Ok(());
    }
    let game = repo.game_dir();
    for u in &ups {
        let name = u.entry.name.as_deref().unwrap_or(&u.path);
        println!("  {name}: {} -> {}", u.current.as_deref().unwrap_or("?"), u.entry.version.as_deref().unwrap_or("?"));
        if apply {
            let folder = Path::new(&u.path).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "mods".into());
            let dest = game.join(&folder).join(&u.filename);
            fetch::download(u.entry.source.as_ref().map(|s| s.url()).unwrap_or_default(), &u.entry.hash, &dest).await?;
            if game.join(&u.path) != dest {
                std::fs::remove_file(game.join(&u.path)).ok();
            }
        }
    }
    if apply {
        repo.stage(ups.into_iter().map(|u| u.entry))?;
        ui::ok("Updated. Review with `beacon status` and save.");
    } else {
        ui::info("Run `beacon updates --apply` to install them.");
    }
    Ok(())
}

pub async fn upstream(dir: &Path, pull: bool) -> Result<()> {
    let repo = find(dir)?;
    let info = ops::check_upstream(&repo).await?;
    if !pull {
        if info.available {
            ui::info(format!("{} has a new version: {}. Run `beacon upstream --pull`.", info.name, info.version));
        } else {
            ui::ok(format!("Up to date with {}", info.name));
        }
        return Ok(());
    }
    let bar = Bar::new();
    let r = ops::pull_upstream(&repo, &|p| bar.update(p)).await?;
    bar.done();
    ui::ok(format!("Merged {} {}. {} conflicts (your files kept). Save to finish.", info.name, info.version, r.conflicts.len()));
    Ok(())
}

pub(crate) fn hub_for(repo: &Repo) -> Result<Hub> {
    let s = Settings::load();
    let mut hub = Hub::authed(&s)?;
    if let Some(r) = repo.config()?.remote {
        hub = Hub::new(&r.hub, s.token);
    }
    Ok(hub)
}

pub async fn push(dir: &Path, force: bool) -> Result<()> {
    let repo = find(dir)?;
    let hub = hub_for(&repo)?;
    ops::ensure_remote(&repo, &hub).await?;
    let id = ops::push(&repo, &hub, force).await?;
    ui::ok(format!("Pushed {}", short(&id)));
    Ok(())
}

pub async fn pull(dir: &Path) -> Result<()> {
    let repo = find(dir)?;
    let hub = hub_for(&repo)?;
    let bar = Bar::new();
    let r = ops::pull(&repo, &hub, &|p| bar.update(p)).await?;
    bar.done();
    match r {
        None => ui::ok("Up to date"),
        Some(m) if m.fast_forward => ui::ok("Pulled new versions"),
        Some(m) if m.conflicts.is_empty() => ui::ok("Merged hub versions. Save to finish."),
        Some(m) => ui::warn(format!("Merged with {} conflicts (your files kept). Save to finish.", m.conflicts.len())),
    }
    Ok(())
}

pub async fn publish(dir: &Path, name: &str, notes: &str) -> Result<()> {
    let repo = find(dir)?;
    let hub = hub_for(&repo)?;
    let rel = ops::publish(&repo, &hub, name, notes).await?;
    let code = repo.config()?.remote.map(|r| r.code).unwrap_or_default();
    ui::ok(format!(
        "Published {} ({}). Share code: {}  Link: {}/j/{code}",
        rel.name,
        short(&rel.commit),
        ui::bold(&code),
        hub.base
    ));
    Ok(())
}

pub async fn deploy(dir: &Path, release: Option<String>, servers: Vec<String>) -> Result<()> {
    let repo = find(dir)?;
    let hub = hub_for(&repo)?;
    let remote = repo.config()?.remote.context("publish the pack first")?;
    let releases = hub.releases(&remote.pack).await?;
    let rel = match &release {
        Some(r) => releases.iter().find(|x| &x.name == r || &x.id == r || x.commit.starts_with(r.as_str())),
        None => releases.first(),
    }
    .context("release not found; publish one with `beacon publish`")?;
    let all = hub.servers(&remote.pack).await?;
    let targets = if servers.is_empty() {
        DeployTargets::All(AllServers::All)
    } else {
        DeployTargets::Some(
            servers
                .iter()
                .map(|s| {
                    all.iter()
                        .find(|x| &x.id == s || &x.name == s)
                        .map(|x| x.id.clone())
                        .with_context(|| format!("no server {s}"))
                })
                .collect::<Result<_>>()?,
        )
    };
    let d = hub.deploy(&remote.pack, &DeployRequest { release: rel.id.clone(), servers: targets }).await?;
    ui::ok(format!("Deploying {} to {} server(s)", rel.name, d.servers.len()));
    follow_deploy(&hub, &remote.pack, &rel.commit).await
}

/// Prints each server's progress until every server sent `commit` is on it, failed or is
/// offline (it updates when it's back). Ctrl+C stops watching; the deploy carries on.
async fn follow_deploy(hub: &Hub, pack: &str, commit: &str) -> Result<()> {
    let mut seen: std::collections::HashMap<String, (String, String)> = Default::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15 * 60);
    while std::time::Instant::now() < deadline {
        let servers = hub.servers(pack).await?;
        let mut waiting = 0;
        for s in servers.iter().filter(|s| s.target_commit.as_deref() == Some(commit)) {
            let arrived = s.current_commit.as_deref() == Some(commit) && s.status != "applying";
            let state = match () {
                _ if !s.online => ("offline".to_string(), "updates when it's back".to_string()),
                _ if arrived => ("done".to_string(), String::new()),
                _ => (s.status.clone(), s.message.clone()),
            };
            if seen.get(&s.id) != Some(&state) {
                match state.0.as_str() {
                    "done" => ui::ok(format!("{} is on the new version", s.name)),
                    "failed" => ui::warn(format!("{}: {}", s.name, s.message)),
                    _ if !state.1.is_empty() => println!("  {} {}", s.name, ui::dim(&state.1)),
                    _ => {}
                }
                seen.insert(s.id.clone(), state.clone());
            }
            if s.online && !arrived && s.status != "failed" {
                waiting += 1;
            }
        }
        if waiting == 0 {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    Ok(())
}

pub async fn update(dir: &Path) -> Result<()> {
    let repo = find(dir)?;
    if repo.config()?.role != Role::Subscriber {
        bail!("`beacon update` is for joined packs; use `beacon pull` for your own");
    }
    let bar = Bar::new();
    let info = ops::update_subscribed(&repo, &|p| bar.update(p)).await?;
    bar.done();
    ui::ok(format!("Up to date ({})", info.name));
    Ok(())
}

pub async fn export(dir: &Path, kind: ExportKind, out: &Path) -> Result<()> {
    let repo = find(dir)?;
    match kind {
        ExportKind::Mrpack => {
            let version = repo.head_commit()?.map(|h| short(&h).to_string()).unwrap_or_else(|| "dev".into());
            import::export_mrpack(&repo, out, &version)?;
        }
        ExportKind::Server => {
            let tree = repo.head_tree()?.context("save a version first")?;
            let name = repo.config()?.name;
            let tmp: PathBuf = instance::data_dir().join("tmp").join(format!("server-{}", std::process::id()));
            let bar = Bar::new();
            server::build_folder(&tmp, &name, &tree, &repo.fetcher(), 4096, &|p| bar.update(p)).await?;
            bar.done();
            server::zip_dir(&tmp, out)?;
            std::fs::remove_dir_all(&tmp).ok();
        }
    }
    ui::ok(format!("Wrote {}", out.display()));
    Ok(())
}

pub async fn play(dir: &Path) -> Result<()> {
    if !prism::has_display() {
        bail!("no display here, so Minecraft can't open; play from a computer with a screen");
    }
    let repo = find(dir)?;
    let server = ops::server_address(&repo).await;
    let bar = Bar::new();
    // The game keeps running after beacon exits.
    drop(prism::launch(&repo.root, &Settings::load(), server.as_deref(), &|p| bar.update(p)).await?);
    bar.done();
    ui::ok("Starting Minecraft");
    Ok(())
}
