//! Comparing two trees by file identity, so a mod updated to a new filename shows as one change.

use crate::model::{Entry, Loader, Tree, is_disabled};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Added,
    Removed,
    /// Same identity, different bytes (new mod version, edited config...).
    Modified,
    /// Only the `.disabled` suffix changed.
    Enabled,
    Disabled,
}

#[derive(Serialize, Clone, Debug)]
pub struct FileRef {
    pub path: String,
    #[serde(flatten)]
    pub entry: Entry,
}

#[derive(Serialize, Clone, Debug)]
pub struct Change {
    pub id: String,
    pub kind: ChangeKind,
    /// Path in the newer tree, or the old path for removals.
    pub path: String,
    pub old: Option<FileRef>,
    pub new: Option<FileRef>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct TreeDiff {
    /// `(old, new)` Minecraft version when it changed.
    pub minecraft: Option<(String, String)>,
    pub loader: Option<(Loader, Loader)>,
    pub files: Vec<Change>,
}

impl TreeDiff {
    pub fn is_empty(&self) -> bool {
        self.minecraft.is_none() && self.loader.is_none() && self.files.is_empty()
    }
}

pub fn diff(old: &Tree, new: &Tree) -> TreeDiff {
    let a = old.by_identity();
    let b = new.by_identity();
    let ids: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    let mut files = Vec::new();
    for id in ids {
        let fr = |(p, e): (&str, &Entry)| FileRef { path: p.to_string(), entry: e.clone() };
        let (o, n) = (a.get(id).copied(), b.get(id).copied());
        let kind = match (o, n) {
            (None, Some(_)) => ChangeKind::Added,
            (Some(_), None) => ChangeKind::Removed,
            (Some((op, oe)), Some((np, ne))) => {
                if !oe.same_content(ne) {
                    ChangeKind::Modified
                } else if op == np {
                    continue;
                } else if is_disabled(np) && !is_disabled(op) {
                    ChangeKind::Disabled
                } else if is_disabled(op) && !is_disabled(np) {
                    ChangeKind::Enabled
                } else {
                    ChangeKind::Modified
                }
            }
            (None, None) => unreachable!(),
        };
        let path = n.or(o).map(|(p, _)| p.to_string()).unwrap_or_default();
        files.push(Change { id: id.clone(), kind, path, old: o.map(fr), new: n.map(fr) });
    }
    TreeDiff {
        minecraft: (old.minecraft != new.minecraft).then(|| (old.minecraft.clone(), new.minecraft.clone())),
        loader: (old.loader != new.loader).then(|| (old.loader.clone(), new.loader.clone())),
        files,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LoaderKind, Source};

    fn modrinth(hash: &str, project: &str) -> Entry {
        Entry {
            source: Some(Source::Modrinth { project: project.into(), version: hash.into(), url: String::new() }),
            ..Entry::bare(hash.into(), 1)
        }
    }

    #[test]
    fn mod_update_with_new_filename_is_one_modification() {
        let loader = Loader { kind: LoaderKind::Fabric, version: Some("0.16".into()) };
        let mut a = Tree::new("1.21.1", loader.clone());
        let mut b = Tree::new("1.21.1", loader);
        a.files.insert("mods/sodium-1.jar".into(), modrinth("aa", "sodium"));
        b.files.insert("mods/sodium-2.jar".into(), modrinth("bb", "sodium"));
        b.files.insert("config/x.toml".into(), Entry::bare("cc".into(), 3));
        let d = diff(&a, &b);
        assert_eq!(d.files.len(), 2);
        let m = d.files.iter().find(|c| c.id == "mr:sodium").unwrap();
        assert_eq!(m.kind, ChangeKind::Modified);
        assert_eq!(m.path, "mods/sodium-2.jar");
    }

    #[test]
    fn disabling_is_detected() {
        let mut a = Tree::new("1.21.1", Loader::default());
        let mut b = a.clone();
        a.files.insert("mods/x.jar".into(), Entry::bare("aa".into(), 1));
        b.files.insert("mods/x.jar.disabled".into(), Entry::bare("aa".into(), 1));
        assert_eq!(diff(&a, &b).files[0].kind, ChangeKind::Disabled);
    }
}
