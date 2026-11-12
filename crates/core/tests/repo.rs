//! Repo lifecycle on a real instance folder (no network: only plain files).

use beacon_core::{
    diff::ChangeKind,
    fetch::no_progress,
    instance,
    model::{Loader, LoaderKind},
    repo::{self, Repo},
};
use std::fs;

fn new_repo() -> (tempfile::TempDir, Repo) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("pack");
    instance::create(&dir, "Pack", "1.21.1", &Loader { kind: LoaderKind::Fabric, version: Some("0.16.9".into()) }).unwrap();
    let repo = Repo::init(&dir, repo::local_config("Pack")).unwrap();
    (tmp, repo)
}

fn write(repo: &Repo, rel: &str, text: &str) {
    let p = repo.game_dir().join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn read(repo: &Repo, rel: &str) -> Option<String> {
    fs::read_to_string(repo.game_dir().join(rel)).ok()
}

#[tokio::test]
async fn save_undo_restore_and_branches() {
    let (_tmp, repo) = new_repo();
    write(&repo, "config/a.toml", "1");
    write(&repo, "saves/world/level.dat", "ignored");
    let v1 = repo.commit("v1", "me").await.unwrap();
    assert_eq!(repo.head_tree().unwrap().unwrap().files.keys().collect::<Vec<_>>(), ["config/a.toml"]);

    // Edits show up as changes and can be undone.
    write(&repo, "config/a.toml", "2");
    write(&repo, "config/b.toml", "new");
    let kinds: Vec<_> = repo.status().unwrap().diff.files.iter().map(|c| c.kind).collect();
    assert_eq!(kinds, [ChangeKind::Modified, ChangeKind::Added]);
    repo.discard(None, &no_progress).await.unwrap();
    assert_eq!(read(&repo, "config/a.toml").as_deref(), Some("1"));
    assert_eq!(read(&repo, "config/b.toml"), None);
    let st = repo.status().unwrap();
    assert!(st.diff.is_empty(), "{:?}", st.diff);
    assert!(repo.commit("noop", "me").await.is_err(), "empty saves are rejected");

    // Restore brings back old files without moving the branch.
    write(&repo, "config/a.toml", "2");
    repo.commit("v2", "me").await.unwrap();
    repo.restore(&v1[..10], &no_progress).await.unwrap();
    assert_eq!(read(&repo, "config/a.toml").as_deref(), Some("1"));
    repo.commit("Restore v1", "me").await.unwrap();
    assert_eq!(repo.log("HEAD", 10).unwrap().len(), 3);

    // Branch, diverge, merge back with a non-conflicting change on each side.
    repo.create_branch("exp", None).unwrap();
    repo.switch("exp", false, &no_progress).await.unwrap();
    write(&repo, "config/exp.toml", "x");
    repo.commit("exp", "me").await.unwrap();
    repo.switch("main", false, &no_progress).await.unwrap();
    assert_eq!(read(&repo, "config/exp.toml"), None);
    write(&repo, "config/main.toml", "y");
    repo.commit("main", "me").await.unwrap();
    let merged = repo.merge("exp", &no_progress).await.unwrap();
    assert!(merged.conflicts.is_empty() && !merged.fast_forward);
    let merge_commit = repo.commit("Merge exp", "me").await.unwrap();
    assert_eq!(repo.read_commit(&merge_commit).unwrap().parents.len(), 2);
    assert_eq!(read(&repo, "config/exp.toml").as_deref(), Some("x"));
    assert_eq!(read(&repo, "config/main.toml").as_deref(), Some("y"));
}

#[tokio::test]
async fn disabling_a_mod_keeps_its_identity() {
    let (_tmp, repo) = new_repo();
    write(&repo, "mods/thing.jar", "jar bytes");
    repo.commit("add", "me").await.unwrap();
    repo.set_enabled("mods/thing.jar", false).unwrap();
    let changes = repo.status().unwrap().diff.files;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ChangeKind::Disabled);
}

#[tokio::test]
async fn toggling_a_mod_twice_is_a_noop() {
    let (_tmp, repo) = new_repo();
    write(&repo, "mods/thing.jar", "jar bytes");
    // A second click arrives with the path the UI saw before the first rename.
    assert_eq!(repo.set_enabled("mods/thing.jar", false).unwrap(), "mods/thing.jar.disabled");
    assert_eq!(repo.set_enabled("mods/thing.jar", false).unwrap(), "mods/thing.jar.disabled");
    assert_eq!(read(&repo, "mods/thing.jar.disabled").as_deref(), Some("jar bytes"));
    // Both spellings on disk is a conflict, not a silent success.
    write(&repo, "mods/thing.jar", "other bytes");
    assert!(repo.set_enabled("mods/thing.jar", false).is_err());
}

#[tokio::test]
async fn launch_rewrites_are_not_changes() {
    let (_tmp, repo) = new_repo();
    write(&repo, "config/iris.properties", "#Iris\n#Tue Nov 10 15:23:57 CET 2026\nenableShaders=false\n");
    write(&repo, "config/sodium-options.json", "{}");
    repo.commit("v1", "me").await.unwrap();

    // The game stamps a new date and refreshes mod caches on launch.
    write(&repo, "config/iris.properties", "#Iris\n#Thu Nov 12 19:43:27 CET 2026\nenableShaders=false\n");
    write(&repo, ".mods/cape-provider/texture-cache/providers.json", "{}");
    write(&repo, "config/mod/texture-cache/a.bin", "x");
    // Files saved before being ignored do not show up as removed.
    write(&repo, ".beaconignore", "sodium-options.json\n");
    let paths: Vec<_> = repo.status().unwrap().diff.files.into_iter().map(|c| c.path).collect();
    assert_eq!(paths, [".beaconignore"]);
    repo.commit("v2", "me").await.unwrap();

    // A real setting change still shows.
    write(&repo, "config/iris.properties", "#Iris\n#Thu Nov 12 19:50:00 CET 2026\nenableShaders=true\n");
    let kinds: Vec<_> = repo.status().unwrap().diff.files.iter().map(|c| c.kind).collect();
    assert_eq!(kinds, [ChangeKind::Modified]);
}

#[tokio::test]
async fn ignored_files_stop_showing_but_stay_on_disk() {
    let (_tmp, repo) = new_repo();
    write(&repo, "config/sodium-options.json", "a");
    write(&repo, "config/weird[1].txt", "a");
    repo.commit("v1", "me").await.unwrap();

    write(&repo, "config/sodium-options.json", "b");
    write(&repo, "config/weird[1].txt", "b");
    repo.ignore(&["config/sodium-options.json".into(), "config/weird[1].txt".into()]).unwrap();
    repo.ignore(&["config/sodium-options.json".into()]).unwrap();
    let paths: Vec<_> = repo.status().unwrap().diff.files.into_iter().map(|c| c.path).collect();
    assert_eq!(paths, [".beaconignore"]);
    assert_eq!(read(&repo, ".beaconignore").as_deref(), Some("/config/sodium-options.json\n/config/weird\\[1].txt\n"));
    assert_eq!(read(&repo, "config/sodium-options.json").as_deref(), Some("b"));

    repo.commit("v2", "me").await.unwrap();
    assert!(!repo.head_tree().unwrap().unwrap().files.contains_key("config/sodium-options.json"));
}
