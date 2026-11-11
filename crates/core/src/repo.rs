//! The `.beacon` repository: commits, branches, status, checkout, merge.
//!
//! Working tree = tracked files under the instance's game dir. Objects live in
//! `.beacon/objects`. Mod jars with a download source are never stored, only referenced.

use crate::{
    diff::{self, TreeDiff},
    fetch::{Fetcher, OnProgress},
    hash::{self, sha512_file},
    instance,
    merge::{self, Conflict, MergeResult},
    model::{Commit, Entry, FORMAT, Side, Tree, client_only_path, identity},
    sources::modrinth,
    store::{ObjectStore, write_atomic},
};
use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Only versioned on this machine.
    #[default]
    Local,
    /// Published to a hub by this user.
    Owner,
    /// Joined with a share code; follows the owner's releases.
    Subscriber,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    pub hub: String,
    pub pack: String,
    pub code: String,
    pub branch: String,
}

/// The pack this one was forked or imported from. `base_tree` is the upstream tree at the
/// last sync, used as the merge base when pulling upstream changes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Upstream {
    Beacon { hub: String, code: String, name: String, commit: String, base_tree: String },
    Modrinth { project: String, name: String, version: String, base_tree: String },
}

impl Upstream {
    pub fn base_tree(&self) -> &str {
        match self {
            Self::Beacon { base_tree, .. } | Self::Modrinth { base_tree, .. } => base_tree,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PackConfig {
    pub format: u32,
    pub name: String,
    #[serde(default)]
    pub role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<Remote>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<Upstream>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Head {
    Branch(String),
    Detached(String),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct IndexEntry {
    size: u64,
    mtime: u128,
    hash: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Status {
    pub branch: Option<String>,
    pub head: Option<String>,
    pub merging: bool,
    pub diff: TreeDiff,
}

#[derive(Serialize, Clone, Debug)]
pub struct BranchInfo {
    pub name: String,
    pub commit: String,
    pub current: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct LogEntry {
    pub id: String,
    #[serde(flatten)]
    pub commit: Commit,
}

/// Paths never tracked. Users extend this with `minecraft/.beaconignore` (gitignore syntax).
pub const DEFAULT_IGNORE: &[&str] = &[
    "saves/",
    "logs/",
    "crash-reports/",
    "screenshots/",
    "backups/",
    ".cache/",
    "cache/",
    "natives/",
    ".fabric/",
    ".mixin.out/",
    "debug/",
    "downloads/",
    "journeymap/data/",
    "XaeroWorldMap/",
    "XaeroWaypoints/",
    "xaero/",
    "frozenlib/",
    ".packignore",
    "server-resource-packs/",
    "local/",
    ".beacon/",
    "usercache.json",
    "usernamecache.json",
    "servers.dat",
    "servers.dat_old",
    "options.txt",
    "optionsof.txt",
    "optionsshaders.txt",
    "realms_persistence.json",
    "command_history.txt",
    "hotbar.nbt",
    "*.log",
    "*.log.gz",
    "*.tmp",
    "*.part",
    "*.beacon-old",
    ".DS_Store",
    "Thumbs.db",
    "mods/.index/",
    "mods/.connector/",
    "mods/beaconsync-companion*.jar",
    "icon.png",
];

#[derive(Clone, Debug)]
pub struct Repo {
    pub root: PathBuf,
    dot: PathBuf,
    pub store: ObjectStore,
}

impl Repo {
    fn at(root: &Path) -> Self {
        let dot = root.join(".beacon");
        Self { root: root.to_path_buf(), store: ObjectStore::new(dot.join("objects")), dot }
    }

    pub fn is_repo(root: &Path) -> bool {
        root.join(".beacon/config.json").is_file()
    }

    pub fn open(root: &Path) -> Result<Self> {
        if !Self::is_repo(root) {
            bail!("{} is not a BeaconSync pack", root.display());
        }
        Ok(Self::at(root))
    }

    /// Creates the `.beacon` folder inside an existing instance.
    pub fn init(root: &Path, config: PackConfig) -> Result<Self> {
        let repo = Self::at(root);
        fs::create_dir_all(repo.dot.join("refs/heads"))?;
        fs::create_dir_all(repo.dot.join("objects"))?;
        repo.save_config(&PackConfig { format: FORMAT, ..config })?;
        if !repo.dot.join("HEAD").exists() {
            fs::write(repo.dot.join("HEAD"), "ref: refs/heads/main\n")?;
        }
        Ok(repo)
    }

    pub fn game_dir(&self) -> PathBuf {
        instance::game_dir(&self.root)
    }

    // ---- small json files -------------------------------------------------------------

    fn read_json<T: DeserializeOwned + Default>(&self, name: &str) -> T {
        fs::read(self.dot.join(name)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn write_json<T: Serialize>(&self, name: &str, value: &T) -> Result<()> {
        write_atomic(&self.dot.join(name), &serde_json::to_vec_pretty(value)?)
    }

    pub fn config(&self) -> Result<PackConfig> {
        Ok(serde_json::from_slice(&fs::read(self.dot.join("config.json"))?)?)
    }

    pub fn save_config(&self, c: &PackConfig) -> Result<()> {
        self.write_json("config.json", c)
    }

    /// Metadata for files the app added (keyed by hash), so they keep their source.
    pub fn stage(&self, entries: impl IntoIterator<Item = Entry>) -> Result<()> {
        let mut staged: HashMap<String, Entry> = self.read_json("staged.json");
        for e in entries {
            staged.insert(e.hash.clone(), e);
        }
        self.write_json("staged.json", &staged)
    }

    // ---- refs ---------------------------------------------------------------------------

    pub fn head(&self) -> Result<Head> {
        let text = fs::read_to_string(self.dot.join("HEAD")).context("missing HEAD")?;
        let text = text.trim();
        Ok(match text.strip_prefix("ref: refs/heads/") {
            Some(b) => Head::Branch(b.to_string()),
            None => Head::Detached(text.to_string()),
        })
    }

    fn ref_path(&self, branch: &str) -> Result<PathBuf> {
        if branch.is_empty() || branch.contains("..") || branch.starts_with('/') || branch.contains('\\') {
            bail!("invalid branch name {branch:?}");
        }
        Ok(self.dot.join("refs/heads").join(branch))
    }

    pub fn branch_commit(&self, branch: &str) -> Result<Option<String>> {
        Ok(fs::read_to_string(self.ref_path(branch)?).ok().map(|s| s.trim().to_string()))
    }

    pub fn set_branch(&self, branch: &str, commit: &str) -> Result<()> {
        write_atomic(&self.ref_path(branch)?, format!("{commit}\n").as_bytes())
    }

    pub fn head_commit(&self) -> Result<Option<String>> {
        match self.head()? {
            Head::Branch(b) => self.branch_commit(&b),
            Head::Detached(c) => Ok(Some(c)),
        }
    }

    /// Moves whatever HEAD points at to `commit`.
    fn advance_head(&self, commit: &str) -> Result<()> {
        match self.head()? {
            Head::Branch(b) => self.set_branch(&b, commit),
            Head::Detached(_) => write_atomic(&self.dot.join("HEAD"), format!("{commit}\n").as_bytes()),
        }
    }

    pub fn branches(&self) -> Result<Vec<BranchInfo>> {
        let current = match self.head()? {
            Head::Branch(b) => Some(b),
            Head::Detached(_) => None,
        };
        let mut out = vec![];
        for e in walkdir::WalkDir::new(self.dot.join("refs/heads")).min_depth(1) {
            let e = e?;
            if !e.file_type().is_file() {
                continue;
            }
            let name = e.path().strip_prefix(self.dot.join("refs/heads"))?.to_string_lossy().replace('\\', "/");
            let commit = fs::read_to_string(e.path())?.trim().to_string();
            out.push(BranchInfo { current: current.as_deref() == Some(&name), name, commit });
        }
        if let Some(c) = current.filter(|c| !out.iter().any(|b| &b.name == c)) {
            out.push(BranchInfo { name: c, commit: String::new(), current: true });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    pub fn create_branch(&self, name: &str, at: Option<&str>) -> Result<()> {
        if self.branch_commit(name)?.is_some() {
            bail!("branch {name} already exists");
        }
        let commit = match at {
            Some(r) => self.resolve(r)?,
            None => self.head_commit()?.context("save a version before creating branches")?,
        };
        self.set_branch(name, &commit)
    }

    pub fn delete_branch(&self, name: &str) -> Result<()> {
        if self.head()? == Head::Branch(name.into()) {
            bail!("cannot delete the current branch");
        }
        fs::remove_file(self.ref_path(name)?).context("no such branch")?;
        Ok(())
    }

    pub fn rename_branch(&self, from: &str, to: &str) -> Result<()> {
        let commit = self.branch_commit(from)?.context("no such branch")?;
        self.set_branch(to, &commit)?;
        fs::remove_file(self.ref_path(from)?)?;
        if self.head()? == Head::Branch(from.into()) {
            fs::write(self.dot.join("HEAD"), format!("ref: refs/heads/{to}\n"))?;
        }
        Ok(())
    }

    /// Resolves a branch name, `HEAD`, or a (prefix of a) commit id.
    pub fn resolve(&self, rev: &str) -> Result<String> {
        if rev == "HEAD" {
            return self.head_commit()?.context("no versions saved yet");
        }
        if let Some(c) = self.branch_commit(rev)? {
            return Ok(c);
        }
        if hash::is_hash(rev) {
            return Ok(rev.to_string());
        }
        if rev.len() >= 4 && rev.bytes().all(|b| b.is_ascii_hexdigit()) {
            let dir = self.dot.join("objects").join(&rev[..2]);
            let matches: Vec<String> = fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| format!("{}{}", &rev[..2], e.file_name().to_string_lossy()))
                .filter(|h| h.starts_with(rev) && self.read_commit(h).is_ok())
                .collect();
            if let [one] = matches.as_slice() {
                return Ok(one.clone());
            }
        }
        bail!("unknown version {rev:?}")
    }

    // ---- objects ------------------------------------------------------------------------

    pub fn read_commit(&self, id: &str) -> Result<Commit> {
        self.store.get_json(id)
    }

    pub fn read_tree(&self, id: &str) -> Result<Tree> {
        self.store.get_json(id)
    }

    pub fn commit_tree(&self, commit: &str) -> Result<Tree> {
        self.read_tree(&self.read_commit(commit)?.tree)
    }

    pub fn head_tree(&self) -> Result<Option<Tree>> {
        self.head_commit()?.map(|c| self.commit_tree(&c)).transpose()
    }

    /// Commits reachable from `from` (any revision), newest first.
    pub fn log(&self, from: &str, limit: usize) -> Result<Vec<LogEntry>> {
        let mut seen = HashSet::new();
        let mut queue = VecDeque::from([self.resolve(from)?]);
        let mut out = vec![];
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Ok(commit) = self.read_commit(&id) else { continue };
            queue.extend(commit.parents.iter().cloned());
            out.push(LogEntry { id, commit });
        }
        out.sort_by_key(|e| std::cmp::Reverse(e.commit.time));
        out.truncate(limit);
        Ok(out)
    }

    pub fn is_ancestor(&self, ancestor: &str, of: &str) -> bool {
        self.log(of, usize::MAX).is_ok_and(|l| l.iter().any(|e| e.id == ancestor))
    }

    pub fn merge_base(&self, a: &str, b: &str) -> Result<Option<String>> {
        let ours: HashSet<String> = self.log(a, usize::MAX)?.into_iter().map(|e| e.id).collect();
        Ok(self.log(b, usize::MAX)?.into_iter().map(|e| e.id).find(|id| ours.contains(id)))
    }

    // ---- working tree -------------------------------------------------------------------

    fn ignore_matcher(&self) -> Result<ignore::gitignore::Gitignore> {
        let game = self.game_dir();
        let mut b = ignore::gitignore::GitignoreBuilder::new(&game);
        for line in DEFAULT_IGNORE {
            b.add_line(None, line)?;
        }
        let custom = game.join(".beaconignore");
        if custom.is_file()
            && let Some(e) = b.add(custom)
        {
            tracing::warn!("bad .beaconignore: {e}");
        }
        Ok(b.build()?)
    }

    /// Tracked files on disk: relative path (with `/`) and absolute path.
    fn tracked_files(&self) -> Result<Vec<(String, PathBuf, fs::Metadata)>> {
        let game = self.game_dir();
        if !game.is_dir() {
            return Ok(vec![]);
        }
        let ig = self.ignore_matcher()?;
        let mut out = vec![];
        let walker = walkdir::WalkDir::new(&game).min_depth(1).into_iter().filter_entry(|e| {
            let rel = e.path().strip_prefix(&game).unwrap_or(e.path());
            !ig.matched_path_or_any_parents(rel, e.file_type().is_dir()).is_ignore()
        });
        for e in walker {
            let e = e?;
            if !e.file_type().is_file() {
                continue;
            }
            let rel = e.path().strip_prefix(&game)?.to_string_lossy().replace('\\', "/");
            out.push((rel, e.path().to_path_buf(), e.metadata()?));
        }
        Ok(out)
    }

    /// Snapshot of the working tree, reusing cached hashes for unchanged files and
    /// carrying over metadata (source, side, names) from HEAD and staged entries.
    pub fn scan(&self) -> Result<Tree> {
        let (minecraft, loader) = instance::read_components(&self.root)?;
        let files = self.tracked_files()?;
        let index: HashMap<String, IndexEntry> = self.read_json("index.json");
        let hashed: Vec<(String, IndexEntry)> = files
            .par_iter()
            .map(|(rel, abs, meta)| {
                let size = meta.len();
                let mtime =
                    meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
                let hash = match index.get(rel) {
                    Some(i) if i.size == size && i.mtime == mtime => i.hash.clone(),
                    _ => sha512_file(abs)?,
                };
                Ok((rel.clone(), IndexEntry { size, mtime, hash }))
            })
            .collect::<std::io::Result<_>>()?;
        let new_index: HashMap<&str, &IndexEntry> = hashed.iter().map(|(p, i)| (p.as_str(), i)).collect();
        self.write_json("index.json", &new_index)?;

        let head = self.head_tree()?.unwrap_or_default();
        let by_hash: HashMap<&str, &Entry> = head.files.values().map(|e| (e.hash.as_str(), e)).collect();
        let staged: HashMap<String, Entry> = self.read_json("staged.json");
        let mut tree = Tree::new(minecraft, loader);
        for (rel, idx) in hashed {
            let known = head
                .files
                .get(&rel)
                .filter(|e| e.hash == idx.hash)
                .or_else(|| by_hash.get(idx.hash.as_str()).copied())
                .or_else(|| staged.get(&idx.hash));
            let entry = match known {
                Some(e) => Entry { size: idx.size, ..e.clone() },
                None => Entry {
                    side: if client_only_path(&rel) { Side::Client } else { Side::Both },
                    ..Entry::bare(idx.hash, idx.size)
                },
            };
            tree.files.insert(rel, entry);
        }
        Ok(tree)
    }

    pub fn status(&self) -> Result<Status> {
        let head = self.head_commit()?;
        let base = head.as_deref().map(|c| self.commit_tree(c)).transpose()?;
        let work = self.scan()?;
        let diff = match &base {
            Some(b) => diff::diff(b, &work),
            None => diff::diff(&Tree::new(work.minecraft.clone(), work.loader.clone()), &work),
        };
        Ok(Status {
            branch: match self.head()? {
                Head::Branch(b) => Some(b),
                Head::Detached(_) => None,
            },
            head,
            merging: self.dot.join("MERGE_HEAD").is_file(),
            diff,
        })
    }

    /// Looks up unknown mod jars on Modrinth by hash (and reads jar metadata for the rest),
    /// staging what it learns so future scans show proper names and sources.
    pub async fn identify(&self, tree: &Tree) -> Result<usize> {
        let staged: HashMap<String, Entry> = self.read_json("staged.json");
        let unknown: Vec<(&String, &Entry)> =
            tree.mods().filter(|(_, e)| e.source.is_none() && e.name.is_none() && !staged.contains_key(&e.hash)).collect();
        if unknown.is_empty() {
            return Ok(0);
        }
        let hashes: Vec<String> = unknown.iter().map(|(_, e)| e.hash.clone()).collect();
        let found = modrinth::identify(&hashes).await.unwrap_or_else(|e| {
            tracing::warn!("modrinth lookup failed: {e:#}");
            HashMap::new()
        });
        let game = self.game_dir();
        let mut learned = vec![];
        for (path, e) in unknown {
            let entry = match found.get(&e.hash) {
                Some(m) => Entry { size: e.size, ..m.clone() },
                None => {
                    let meta = read_jar_meta(&game.join(path)).unwrap_or_default();
                    Entry {
                        name: meta.name.or_else(|| Some(jar_stem(path))),
                        version: meta.version,
                        side: meta.side.unwrap_or(e.side),
                        ..e.clone()
                    }
                }
            };
            learned.push(entry);
        }
        let n = learned.len();
        self.stage(learned)?;
        Ok(n)
    }

    /// Saves the working tree as a new version on the current branch.
    pub async fn commit(&self, message: &str, author: &str) -> Result<String> {
        let first = self.scan()?;
        self.identify(&first).await?;
        let tree = self.scan()?;
        let game = self.game_dir();
        for (path, e) in &tree.files {
            if e.source.is_none() {
                self.store.put_file(&e.hash, &game.join(path))?;
            }
        }
        let tree_id = self.store.put_json(&tree)?;
        let head = self.head_commit()?;
        let merge_head = fs::read_to_string(self.dot.join("MERGE_HEAD")).ok().map(|s| s.trim().to_string());
        if merge_head.is_none()
            && let Some(h) = &head
            && self.read_commit(h)?.tree == tree_id
        {
            bail!("nothing changed since the last version");
        }
        let commit = Commit {
            format: FORMAT,
            tree: tree_id,
            parents: head.into_iter().chain(merge_head).collect(),
            author: author.to_string(),
            message: message.trim().to_string(),
            time: crate::now(),
        };
        let id = self.store.put_json(&commit)?;
        self.advance_head(&id)?;
        fs::remove_file(self.dot.join("MERGE_HEAD")).ok();
        self.finish_upstream_sync()?;
        // Staged metadata is now in HEAD; keep only entries not yet committed.
        let mut staged: HashMap<String, Entry> = self.read_json("staged.json");
        staged.retain(|h, _| !tree.files.values().any(|e| &e.hash == h));
        self.write_json("staged.json", &staged)?;
        Ok(id)
    }

    pub fn fetcher(&self) -> Fetcher {
        let cfg = self.config().ok();
        let hub = cfg.as_ref().and_then(|c| {
            c.remote.as_ref().map(|r| r.hub.clone()).or_else(|| match &c.upstream {
                Some(Upstream::Beacon { hub, .. }) => Some(hub.clone()),
                _ => None,
            })
        });
        Fetcher::new(Some(self.store.clone()), hub)
    }

    /// Makes the working tree match `target`. With `only`, touches just those identities.
    pub async fn checkout_tree(&self, target: &Tree, only: Option<&HashSet<String>>, on: OnProgress<'_>) -> Result<()> {
        target.validate()?;
        let current = self.scan()?;
        let game = self.game_dir();
        let wanted = |path: &str, e: &Entry| only.is_none_or(|ids| ids.contains(&identity(path, e)));
        let mut fetch = vec![];
        for (path, e) in &target.files {
            if wanted(path, e) && current.files.get(path).is_none_or(|c| c.hash != e.hash) {
                fetch.push((e.clone(), game.join(path)));
            }
        }
        self.stage(target.files.values().cloned())?;
        self.fetcher().fetch_many(fetch, on).await?;
        for (path, e) in &current.files {
            if wanted(path, e) && !target.files.contains_key(path) {
                fs::remove_file(game.join(path)).ok();
            }
        }
        if only.is_none() && (current.minecraft != target.minecraft || current.loader != target.loader) {
            instance::write_components(&self.root, &target.minecraft, &target.loader)?;
        }
        prune_empty_dirs(&game);
        Ok(())
    }

    /// Throws away unsaved changes (all, or just the given identities).
    pub async fn discard(&self, only: Option<&HashSet<String>>, on: OnProgress<'_>) -> Result<()> {
        let tree = self.head_tree()?.context("no saved version to go back to")?;
        self.checkout_tree(&tree, only, on).await
    }

    /// Puts an older version's files in the working tree without moving the branch.
    /// Saving afterwards records the rollback as a new version.
    pub async fn restore(&self, rev: &str, on: OnProgress<'_>) -> Result<()> {
        let tree = self.commit_tree(&self.resolve(rev)?)?;
        self.checkout_tree(&tree, None, on).await
    }

    /// Moves the current branch to `rev` and checks it out (drops newer versions).
    pub async fn reset_hard(&self, rev: &str, on: OnProgress<'_>) -> Result<()> {
        let id = self.resolve(rev)?;
        let tree = self.commit_tree(&id)?;
        self.checkout_tree(&tree, None, on).await?;
        self.advance_head(&id)
    }

    pub fn is_dirty(&self) -> Result<bool> {
        Ok(!self.status()?.diff.is_empty())
    }

    /// Switches to another branch (or a commit, detached).
    pub async fn switch(&self, target: &str, force: bool, on: OnProgress<'_>) -> Result<()> {
        if !force && self.is_dirty()? {
            bail!("you have unsaved changes; save or discard them first");
        }
        let (head_text, commit) = match self.branch_commit(target)? {
            Some(c) => (format!("ref: refs/heads/{target}\n"), c),
            None => {
                let c = self.resolve(target)?;
                (format!("{c}\n"), c)
            }
        };
        let tree = self.commit_tree(&commit)?;
        self.checkout_tree(&tree, None, on).await?;
        write_atomic(&self.dot.join("HEAD"), head_text.as_bytes())
    }

    /// Merges a branch (or commit) into the working tree. Conflicts keep the current files and
    /// are returned for the user to resolve; the next commit records both parents.
    pub async fn merge(&self, rev: &str, on: OnProgress<'_>) -> Result<MergeResult> {
        if self.is_dirty()? {
            bail!("save or discard your changes before merging");
        }
        let theirs_id = self.resolve(rev)?;
        let ours_id = self.head_commit()?.context("nothing saved on this branch yet")?;
        if self.is_ancestor(&theirs_id, &ours_id) {
            bail!("already up to date");
        }
        let theirs = self.commit_tree(&theirs_id)?;
        if self.is_ancestor(&ours_id, &theirs_id) {
            self.checkout_tree(&theirs, None, on).await?;
            self.advance_head(&theirs_id)?;
            return Ok(MergeResult { tree: theirs, conflicts: vec![], fast_forward: true });
        }
        let base = match self.merge_base(&ours_id, &theirs_id)? {
            Some(b) => self.commit_tree(&b)?,
            None => Tree::default(),
        };
        let result = merge::merge(&base, &self.commit_tree(&ours_id)?, &theirs);
        self.checkout_tree(&result.tree, None, on).await?;
        fs::write(self.dot.join("MERGE_HEAD"), &theirs_id)?;
        Ok(result)
    }

    pub fn abort_merge(&self) -> Result<()> {
        fs::remove_file(self.dot.join("MERGE_HEAD")).ok();
        fs::remove_file(self.dot.join("UPSTREAM_PENDING")).ok();
        Ok(())
    }

    /// Applies conflict picks to the working tree.
    pub async fn resolve_conflicts(
        &self,
        conflicts: &[Conflict],
        picks: &[(String, merge::Pick)],
        on: OnProgress<'_>,
    ) -> Result<()> {
        let mut tree = self.scan()?;
        let result = MergeResult { tree: tree.clone(), conflicts: conflicts.to_vec(), fast_forward: false };
        tree = merge::resolve(result, picks);
        self.checkout_tree(&tree, None, on).await
    }

    /// Merges a new upstream tree into the working tree. The upstream base moves forward
    /// once the user saves the result.
    pub async fn merge_upstream(&self, theirs: &Tree, new_upstream: Upstream, on: OnProgress<'_>) -> Result<MergeResult> {
        let cfg = self.config()?;
        let up = cfg.upstream.as_ref().context("this pack has no upstream")?;
        let base = self.read_tree(up.base_tree()).unwrap_or_default();
        let ours = self.scan()?;
        let result = merge::merge(&base, &ours, theirs);
        self.checkout_tree(&result.tree, None, on).await?;
        self.write_json("UPSTREAM_PENDING", &new_upstream)?;
        Ok(result)
    }

    /// Upstream state merged into the working tree but not saved yet.
    pub fn pending_upstream(&self) -> Option<Upstream> {
        serde_json::from_slice(&fs::read(self.dot.join("UPSTREAM_PENDING")).ok()?).ok()
    }

    fn finish_upstream_sync(&self) -> Result<()> {
        let path = self.dot.join("UPSTREAM_PENDING");
        if let Ok(bytes) = fs::read(&path) {
            let up: Upstream = serde_json::from_slice(&bytes)?;
            let mut cfg = self.config()?;
            cfg.upstream = Some(up);
            self.save_config(&cfg)?;
            fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Writes a downloaded tree + its commit into the local store (used by fetch and joins).
    pub fn store_tree(&self, tree: &Tree) -> Result<String> {
        self.store.put_json(tree)
    }

    /// Adds files the app downloaded (e.g. a mod from Modrinth) with their metadata.
    pub fn add_file(&self, rel: &str, bytes: &[u8], mut entry: Entry) -> Result<()> {
        entry.hash = hash::sha512(bytes);
        entry.size = bytes.len() as u64;
        write_atomic(&self.game_dir().join(rel), bytes)?;
        self.stage([entry])
    }

    /// Toggles a mod on or off by renaming it with the `.disabled` suffix.
    /// `rel` may be either spelling; a mod already in the requested state is left alone,
    /// so a repeated toggle from a stale UI row is a no-op instead of an error.
    pub fn set_enabled(&self, rel: &str, enabled: bool) -> Result<String> {
        let game = self.game_dir();
        let base = crate::model::enabled_path(rel);
        let target = if enabled { base.to_string() } else { format!("{base}.disabled") };
        if target == rel {
            return Ok(target);
        }
        match (game.join(rel).exists(), game.join(&target).exists()) {
            (true, true) => bail!("both {rel} and {target} exist, remove one of them"),
            (_, true) => {}
            _ => fs::rename(game.join(rel), game.join(&target))?,
        }
        Ok(target)
    }

    /// Overrides which side (client/server/both) a file belongs to.
    pub fn set_side(&self, rel: &str, side: Side) -> Result<()> {
        let tree = self.scan()?;
        let e = tree.files.get(rel).context("no such file")?;
        self.stage([Entry { side, ..e.clone() }])
    }

    pub fn remove_file(&self, rel: &str) -> Result<()> {
        fs::remove_file(self.game_dir().join(rel))?;
        Ok(())
    }

    /// Commit ids not yet stored as objects (used before pushing).
    pub fn objects_for(&self, commits: &[String]) -> Result<Vec<String>> {
        let mut out = vec![];
        for c in commits {
            let commit = self.read_commit(c)?;
            let tree = self.read_tree(&commit.tree)?;
            out.push(c.clone());
            out.push(commit.tree.clone());
            out.extend(tree.files.values().filter(|e| e.source.is_none()).map(|e| e.hash.clone()));
        }
        out.sort();
        out.dedup();
        Ok(out)
    }
}

fn prune_empty_dirs(root: &Path) {
    for e in walkdir::WalkDir::new(root).min_depth(1).contents_first(true).into_iter().flatten() {
        if e.file_type().is_dir() && !e.path().ends_with("mods") && !e.path().ends_with("config") {
            fs::remove_dir(e.path()).ok();
        }
    }
}

fn jar_stem(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    crate::model::enabled_path(file).trim_end_matches(".jar").to_string()
}

#[derive(Default, Debug)]
pub struct JarMeta {
    pub name: Option<String>,
    pub version: Option<String>,
    pub side: Option<Side>,
}

/// Reads display name, version and environment from a mod jar
/// (fabric.mod.json, quilt.mod.json or (neo)forge mods.toml).
pub fn read_jar_meta(path: &Path) -> Result<JarMeta> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
    let mut read = |name: &str| -> Option<String> {
        let mut s = String::new();
        zip.by_name(name).ok()?.read_to_string(&mut s).ok()?;
        Some(s)
    };
    if let Some(text) = read("fabric.mod.json") {
        let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        let side = match v["environment"].as_str() {
            Some("client") => Some(Side::Client),
            Some("server") => Some(Side::Server),
            _ => None,
        };
        let s = |k: &str| v[k].as_str().map(str::to_string);
        return Ok(JarMeta { name: s("name").or_else(|| s("id")), version: s("version"), side });
    }
    if let Some(text) = read("quilt.mod.json") {
        let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        let q = &v["quilt_loader"];
        let side = match v["minecraft"]["environment"].as_str() {
            Some("client") => Some(Side::Client),
            Some("dedicated_server") => Some(Side::Server),
            _ => None,
        };
        return Ok(JarMeta {
            name: q["metadata"]["name"].as_str().or(q["id"].as_str()).map(str::to_string),
            version: q["version"].as_str().map(str::to_string),
            side,
        });
    }
    let toml = read("META-INF/neoforge.mods.toml").or_else(|| read("META-INF/mods.toml"));
    if let Some(text) = toml {
        let field = |key: &str| {
            text.lines().find_map(|l| {
                let (k, v) = l.split_once('=')?;
                (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
            })
        };
        let version = field("version").filter(|v| !v.contains("${")).or_else(|| {
            read("META-INF/MANIFEST.MF")?
                .lines()
                .find_map(|l| l.strip_prefix("Implementation-Version:").map(|v| v.trim().to_string()))
        });
        return Ok(JarMeta { name: field("displayName"), version, side: None });
    }
    Ok(JarMeta::default())
}

/// Builds `PackConfig` for a brand new local pack.
pub fn local_config(name: &str) -> PackConfig {
    PackConfig { format: FORMAT, name: name.to_string(), ..Default::default() }
}
