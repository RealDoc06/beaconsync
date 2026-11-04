//! Three-way merge of trees, keyed by file identity. Used for branch merges and for pulling
//! an upstream pack into a fork.

use crate::model::{Entry, Tree};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

type Side = Option<(String, Entry)>;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Conflict {
    pub id: String,
    pub base: Side,
    pub ours: Side,
    pub theirs: Side,
}

#[derive(Serialize, Clone, Debug)]
pub struct MergeResult {
    /// Merged tree. Conflicting files keep "ours" until resolved.
    pub tree: Tree,
    pub conflicts: Vec<Conflict>,
    /// True when the branch simply moved forward (nothing to save).
    pub fast_forward: bool,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Pick {
    Ours,
    Theirs,
}

fn same(a: &Side, b: &Side) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some((pa, ea)), Some((pb, eb))) => pa == pb && ea.same_content(eb),
        _ => false,
    }
}

fn pick3<T: PartialEq + Clone>(base: &T, ours: &T, theirs: &T) -> T {
    if ours == theirs || theirs == base { ours.clone() } else { theirs.clone() }
}

pub fn merge(base: &Tree, ours: &Tree, theirs: &Tree) -> MergeResult {
    let (b, o, t) = (base.by_identity(), ours.by_identity(), theirs.by_identity());
    let owned = |m: &std::collections::BTreeMap<String, (&str, &Entry)>, id: &String| -> Side {
        m.get(id).map(|(p, e)| (p.to_string(), (*e).clone()))
    };
    let ids: BTreeSet<&String> = b.keys().chain(o.keys()).chain(t.keys()).collect();
    let mut tree =
        Tree::new(pick3(&base.minecraft, &ours.minecraft, &theirs.minecraft), pick3(&base.loader, &ours.loader, &theirs.loader));
    let mut conflicts = Vec::new();
    for id in ids {
        let (bs, os, ts) = (owned(&b, id), owned(&o, id), owned(&t, id));
        let chosen = if same(&os, &ts) || same(&ts, &bs) {
            os
        } else if same(&os, &bs) {
            ts
        } else {
            conflicts.push(Conflict { id: id.clone(), base: bs, ours: os.clone(), theirs: ts });
            os
        };
        if let Some((p, e)) = chosen {
            tree.files.insert(p, e);
        }
    }
    MergeResult { tree, conflicts, fast_forward: false }
}

/// Applies conflict choices on top of a merge result.
pub fn resolve(mut result: MergeResult, picks: &[(String, Pick)]) -> Tree {
    for (id, pick) in picks {
        let Some(c) = result.conflicts.iter().find(|c| &c.id == id) else { continue };
        if let Some((p, _)) = &c.ours {
            result.tree.files.remove(p);
        }
        let side = if *pick == Pick::Ours { &c.ours } else { &c.theirs };
        if let Some((p, e)) = side {
            result.tree.files.insert(p.clone(), e.clone());
        }
    }
    result.tree
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Loader;

    fn tree(files: &[(&str, &str)]) -> Tree {
        let mut t = Tree::new("1.21.1", Loader::default());
        for (p, h) in files {
            t.files.insert(p.to_string(), Entry::bare(h.to_string(), 1));
        }
        t
    }

    #[test]
    fn takes_non_conflicting_changes_from_both_sides() {
        let base = tree(&[("config/a", "1"), ("config/b", "1")]);
        let ours = tree(&[("config/a", "2"), ("config/b", "1"), ("config/mine", "1")]);
        let theirs = tree(&[("config/a", "1"), ("config/theirs", "1")]);
        let r = merge(&base, &ours, &theirs);
        assert!(r.conflicts.is_empty());
        let paths: Vec<_> = r.tree.files.keys().map(String::as_str).collect();
        assert_eq!(paths, ["config/a", "config/mine", "config/theirs"]);
        assert_eq!(r.tree.files["config/a"].hash, "2");
    }

    #[test]
    fn conflicting_edits_keep_ours_until_resolved() {
        let base = tree(&[("config/a", "1")]);
        let r = merge(&base, &tree(&[("config/a", "2")]), &tree(&[("config/a", "3")]));
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(r.tree.files["config/a"].hash, "2");
        let id = r.conflicts[0].id.clone();
        let t = resolve(r, &[(id, Pick::Theirs)]);
        assert_eq!(t.files["config/a"].hash, "3");
    }
}
