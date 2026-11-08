//! Minecraft accounts in Prism's `accounts.json`. BeaconSync lists them for every Prism and, for
//! the built-in one, also adds and removes them so players never open Prism's window.
//! Only profile names leave the file; tokens are copied between Prism files as they are.

use super::{builtin, busy, uses_builtin};
use crate::{fetch::OnProgress, instance::prism_data_dir, settings::Settings, store::write_atomic};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, time::Duration};

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
/// window first.
pub async fn sign_in(s: &Settings, on: OnProgress<'_>) -> Result<Option<Account>> {
    ensure!(uses_builtin(s), "Accounts of an installed Prism Launcher are managed in Prism");
    ensure!(!busy(), "Close Minecraft before adding an account");
    let exe = builtin::ensure(on).await?;
    let temp = builtin::root().join("sign-in");
    fs::remove_dir_all(&temp).ok();
    let mut child = builtin::command(&exe, &temp, &temp.join("instances"))?.spawn().context("starting Prism Launcher")?;
    let signed_in = loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let exited = child.try_wait()?.is_some();
        // Prism saves the account with QSaveFile, so a read never sees half a file.
        let raw = fs::read(temp.join("accounts.json")).unwrap_or_default();
        if !parse(&raw).is_empty() {
            child.kill().ok();
            child.wait().ok();
            break Some(raw);
        }
        if exited {
            break None;
        }
    };
    let added = signed_in.map(|raw| -> Result<Account> {
        let path = builtin::data().join("accounts.json");
        let merged = merge(fs::read(&path).ok().as_deref(), &raw)?;
        write_atomic(&path, &merged)?;
        parse(&raw).into_iter().next().context("no account")
    });
    fs::remove_dir_all(&temp).ok();
    added.transpose()
}

/// Removes an account from the built-in Prism.
pub fn remove(s: &Settings, name: &str) -> Result<()> {
    ensure!(uses_builtin(s), "Accounts of an installed Prism Launcher are managed in Prism");
    ensure!(!busy(), "Close Minecraft before removing an account");
    let path = builtin::data().join("accounts.json");
    let mut root: Value = serde_json::from_slice(&fs::read(&path)?)?;
    let accounts = root["accounts"].as_array_mut().context("accounts.json has no account list")?;
    accounts.retain(|a| a["profile"]["name"] != name);
    ensure_default(accounts);
    write_atomic(&path, &serde_json::to_vec_pretty(&root)?)
}

/// `existing` accounts.json plus every account in `added`, replacing ones with the same profile.
fn merge(existing: Option<&[u8]>, added: &[u8]) -> Result<Vec<u8>> {
    let mut root: Value =
        existing.and_then(|b| serde_json::from_slice(b).ok()).unwrap_or_else(|| json!({ "formatVersion": 3, "accounts": [] }));
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
}
