//! Minecraft accounts in Prism's `accounts.json`. BeaconSync lists them for every Prism and, for
//! the built-in one, also adds and removes them so players never open Prism's window.
//! Only profile names leave the file; tokens are copied between Prism files as they are.

use super::{builtin, busy, uses_builtin};
use crate::{fetch::OnProgress, instance::prism_data_dir, settings::Settings, store::write_private};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Child, sync::atomic::Ordering, time::Duration};

/// A Minecraft account signed in to Prism.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// Minecraft profile name; Prism's `--profile` takes it.
    pub name: String,
    pub offline: bool,
    /// Prism's default account, used when BeaconSync has none picked.
    pub default: bool,
}

/// `accounts.json` of the Prism that plays the game.
fn file(s: &Settings) -> Option<PathBuf> {
    let data = if uses_builtin(s) { Some(builtin::data()) } else { prism_data_dir() };
    data.map(|d| d.join("accounts.json"))
}

/// Accounts in the Prism that plays the game. Accounts without a Minecraft profile (no game
/// bought yet) are left out since Prism can't launch with them.
pub fn list(s: &Settings) -> Vec<Account> {
    file(s).and_then(|f| fs::read(f).ok()).map(|b| parse(&b)).unwrap_or_default()
}

fn parse(json: &[u8]) -> Vec<Account> {
    #[derive(Deserialize)]
    struct File {
        #[serde(default)]
        accounts: Vec<Entry>,
    }
    #[derive(Deserialize)]
    struct Entry {
        #[serde(rename = "type")]
        kind: String,
        #[serde(default)]
        active: bool,
        profile: Option<Profile>,
    }
    #[derive(Deserialize)]
    struct Profile {
        name: String,
    }
    let Ok(file) = serde_json::from_slice::<File>(json) else { return vec![] };
    file.accounts
        .into_iter()
        .filter_map(|e| {
            let name = e.profile?.name;
            (!name.is_empty()).then(|| Account { name, offline: e.kind == "Offline", default: e.active })
        })
        .collect()
}

/// Signs a Microsoft account in to the built-in Prism, showing only Prism's sign-in page: a
/// throwaway Prism without accounts opens straight on it, and once that Prism has saved the
/// account BeaconSync closes it and moves the account over. None when the player closed the
/// window first. Play waits for this to finish (see [`super::signing_in`]).
pub async fn sign_in(s: &Settings, on: OnProgress<'_>) -> Result<Option<Account>> {
    ensure!(uses_builtin(s), "Accounts of an installed Prism Launcher are managed in Prism");
    // Flag first, then check: Play counts itself first, then checks the flag.
    let mut guard = SignIn::start()?;
    ensure!(!busy(), "Close Minecraft before adding an account");
    let exe = builtin::ensure(on).await?;
    let temp = guard.temp.clone();
    guard.child = Some(builtin::command(&exe, &temp, &temp.join("instances"))?.spawn().context("starting Prism Launcher")?);
    let raw = loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        // Prism saves the account with QSaveFile, so a read never sees half a file.
        let raw = fs::read(temp.join("accounts.json")).unwrap_or_default();
        if !parse(&raw).is_empty() {
            break raw;
        }
        if guard.child.as_mut().and_then(|c| c.try_wait().ok()).flatten().is_some() {
            return Ok(None);
        }
    };
    guard.close();
    // Still flagged as signing in, so no Play starts a Prism before the account lands.
    let path = builtin::data().join("accounts.json");
    write_private(&path, &merge(read_if_exists(&path)?.as_deref(), &raw)?)?;
    Ok(parse(&raw).into_iter().next())
}

/// The file's bytes, None when it doesn't exist; other read errors stop the caller.
fn read_if_exists(path: &std::path::Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("reading Prism's accounts.json"),
    }
}

/// One sign-in at a time. Dropping it closes the throwaway Prism and deletes its folder (and
/// the tokens in it) on every exit path.
struct SignIn {
    child: Option<Child>,
    temp: PathBuf,
}

impl SignIn {
    fn start() -> Result<Self> {
        ensure!(!super::SIGNING_IN.swap(true, Ordering::SeqCst), "An account is already being added");
        let temp = builtin::root().join("sign-in");
        let guard = Self { child: None, temp };
        if guard.temp.exists() {
            fs::remove_dir_all(&guard.temp).context("removing an old sign-in folder")?;
        }
        Ok(guard)
    }
}

impl SignIn {
    /// Closes the sign-in Prism (it already saved the account).
    fn close(&mut self) {
        if let Some(mut child) = self.child.take() {
            child.kill().ok();
            child.wait().ok();
        }
    }
}

impl Drop for SignIn {
    fn drop(&mut self) {
        self.close();
        fs::remove_dir_all(&self.temp).ok();
        super::SIGNING_IN.store(false, Ordering::SeqCst);
    }
}

/// Removes an account from the built-in Prism.
pub fn remove(s: &Settings, name: &str) -> Result<()> {
    ensure!(uses_builtin(s), "Accounts of an installed Prism Launcher are managed in Prism");
    ensure!(!busy(), "Close Minecraft before removing an account");
    ensure!(!super::SIGNING_IN.load(Ordering::SeqCst), "Wait for the account being added");
    let path = builtin::data().join("accounts.json");
    let mut root: Value = serde_json::from_slice(&fs::read(&path)?)?;
    let accounts = root["accounts"].as_array_mut().context("accounts.json has no account list")?;
    accounts.retain(|a| a["profile"]["name"] != name);
    ensure_default(accounts);
    write_private(&path, &serde_json::to_vec_pretty(&root)?)
}

/// `existing` accounts.json plus every account in `added`, replacing ones with the same profile.
/// An unreadable existing file is an error rather than replaced, so no account is lost.
fn merge(existing: Option<&[u8]>, added: &[u8]) -> Result<Vec<u8>> {
    let mut root: Value = match existing {
        Some(bytes) => serde_json::from_slice(bytes).context("Prism's accounts.json is unreadable")?,
        None => json!({ "formatVersion": 3, "accounts": [] }),
    };
    let added: Value = serde_json::from_slice(added)?;
    let accounts = root["accounts"].as_array_mut().context("accounts.json has no account list")?;
    for mut a in added["accounts"].as_array().cloned().unwrap_or_default() {
        accounts.retain(|e| e["profile"]["id"] != a["profile"]["id"]);
        if let Some(o) = a.as_object_mut() {
            o.remove("active");
        }
        accounts.push(a);
    }
    ensure_default(accounts);
    Ok(serde_json::to_vec_pretty(&root)?)
}

/// Makes the first account Prism's default when none is.
fn ensure_default(accounts: &mut [Value]) {
    if !accounts.iter().any(|a| a["active"] == true)
        && let Some(o) = accounts.first_mut().and_then(Value::as_object_mut)
    {
        o.insert("active".into(), true.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(name: &str, id: &str, active: bool) -> Value {
        json!({ "type": "MSA", "active": active, "msa": { "token": "t" }, "profile": { "id": id, "name": name } })
    }

    #[test]
    fn parse_keeps_names_and_the_default() {
        let json = br#"{"formatVersion": 3, "accounts": [
            {"type": "MSA", "active": true, "msa": {"token": "secret"}, "profile": {"id": "1", "name": "Steve"}},
            {"type": "Offline", "profile": {"id": "2", "name": "Alex"}},
            {"type": "MSA", "profile": {"id": "", "name": ""}}
        ]}"#;
        let a = |name: &str, offline, default| Account { name: name.into(), offline, default };
        assert_eq!(parse(json), [a("Steve", false, true), a("Alex", true, false)]);
        assert!(parse(b"not json").is_empty());
    }

    #[test]
    fn merge_replaces_the_same_profile_and_keeps_tokens() {
        let existing = json!({ "formatVersion": 3, "accounts": [account("Steve", "1", true), account("Alex", "2", false)] });
        let added = json!({ "accounts": [account("Alex2", "2", true)] });
        let merged: Value = serde_json::from_slice(
            &merge(Some(&serde_json::to_vec(&existing).unwrap()), &serde_json::to_vec(&added).unwrap()).unwrap(),
        )
        .unwrap();
        let names: Vec<_> = merged["accounts"].as_array().unwrap().iter().map(|a| a["profile"]["name"].clone()).collect();
        assert_eq!(names, [json!("Steve"), json!("Alex2")]);
        assert_eq!(merged["accounts"][0]["active"], true);
        assert_eq!(merged["accounts"][1].get("active"), None);
        assert_eq!(merged["accounts"][1]["msa"]["token"], "t");
    }

    #[test]
    fn first_account_becomes_the_default() {
        let added = json!({ "accounts": [account("Steve", "1", false)] });
        let merged: Value = serde_json::from_slice(&merge(None, &serde_json::to_vec(&added).unwrap()).unwrap()).unwrap();
        assert_eq!(merged["formatVersion"], 3);
        assert_eq!(merged["accounts"][0]["active"], true);
    }

    #[test]
    fn merge_refuses_an_unreadable_file() {
        let added = serde_json::to_vec(&json!({ "accounts": [account("Steve", "1", false)] })).unwrap();
        assert!(merge(Some(b"{ truncated"), &added).is_err());
    }
}
