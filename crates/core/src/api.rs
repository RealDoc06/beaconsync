//! Hub API types shared by `beacon-hub` and its clients. See `docs/ARCHITECTURE.md`.

use crate::model::Loader;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct User {
    pub id: String,
    pub username: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuthResponse {
    pub token: String,
    pub user: User,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeviceStart {
    pub device_code: String,
    pub user_code: String,
    pub expires_in: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DevicePoll {
    pub device_code: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeviceApprove {
    pub user_code: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Pack {
    pub id: String,
    pub code: String,
    pub name: String,
    pub description: String,
    pub owner: String,
    pub minecraft: String,
    pub loader: Loader,
    pub created: i64,
    pub updated: i64,
    /// True when the requesting user owns the pack.
    #[serde(default)]
    pub mine: bool,
    #[serde(default)]
    pub latest: Option<Release>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NewPack {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub minecraft: String,
    pub loader: Loader,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PackPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub minecraft: Option<String>,
    pub loader: Option<Loader>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Release {
    pub id: String,
    pub commit: String,
    pub name: String,
    pub notes: String,
    pub created: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NewRelease {
    pub commit: String,
    pub name: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RefUpdate {
    pub old: Option<String>,
    pub new: String,
    #[serde(default)]
    pub force: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServerMode {
    /// Folder with start.sh, run by hand.
    Plain,
    /// systemd service running `beacon server run` (auto start, auto restart, auto update).
    Service,
    /// docker compose service running `beacon server run` inside a Java container.
    Docker,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub mode: ServerMode,
    pub address: String,
    pub current_commit: Option<String>,
    pub target_commit: Option<String>,
    pub target_release: Option<String>,
    /// idle | applying | done | failed | offline
    pub status: String,
    pub message: String,
    pub last_seen: Option<i64>,
    pub online: bool,
    /// Version of `beacon` the server runs; unknown for agents older than 0.1.0-alpha.2.
    #[serde(default)]
    pub agent_version: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NewServer {
    pub name: String,
    pub mode: ServerMode,
    pub address: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NewServerResponse {
    pub server: Server,
    pub token: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum DeployTargets {
    All(AllServers),
    Some(Vec<String>),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum AllServers {
    All,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeployRequest {
    pub release: String,
    pub servers: DeployTargets,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeployResponse {
    pub deployment: String,
    pub servers: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PublicServer {
    pub name: String,
    pub address: String,
    pub commit: Option<String>,
    pub online: bool,
    /// A deploy to `target` is pending or applying (the companion rejoins afterwards).
    #[serde(default)]
    pub updating: bool,
    #[serde(default)]
    pub target: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PublicPack {
    pub id: String,
    pub code: String,
    pub name: String,
    pub owner: String,
    pub description: String,
    pub minecraft: String,
    pub loader: Loader,
    pub latest: Option<Release>,
    pub servers: Vec<PublicServer>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentTarget {
    pub commit: String,
    pub release: String,
    pub release_name: String,
    pub deployment: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentPoll {
    pub target: Option<AgentTarget>,
    /// Hub's idea of the server; lets the agent show its own name.
    pub server: Server,
    pub pack_code: String,
    /// Things the owner asked for from the app or CLI, oldest first.
    #[serde(default)]
    pub actions: Vec<ServerAction>,
}

/// What an owner can ask a running server to do. Relayed by the hub to the agent.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", content = "arg", rename_all = "snake_case")]
pub enum ServerAction {
    /// A Minecraft console command, e.g. `whitelist add Steve`.
    Command(String),
    Start,
    Stop,
    Restart,
    /// Back up the whole server now.
    Backup,
    /// Restore a backup by id (stops and restarts Minecraft).
    Restore(String),
    DeleteBackup(String),
    /// Update `beacon` itself to this BeaconSync version; Minecraft restarts.
    Update(String),
}

/// A backup on a server: BeaconSync's own or one written by a backup mod (FTB Backups...).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Backup {
    /// Path relative to the server folder.
    pub id: String,
    /// "BeaconSync" or the backup mod's name.
    pub source: String,
    /// What it was taken for, e.g. "before 1.4" or "manual".
    pub note: String,
    pub size: u64,
    pub created: i64,
    /// True for whole server backups; false when the archive only holds the world.
    pub full: bool,
}

/// Console output and state an agent uploads.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AgentConsole {
    pub lines: Vec<String>,
    /// Sent when the list changed (and on start).
    pub backups: Option<Vec<Backup>>,
    /// Whether Minecraft is running, when it changed.
    pub running: Option<bool>,
}

/// A slice of a server's console for viewers. `next` is the cursor for the next call.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ConsoleView {
    pub lines: Vec<String>,
    pub next: u64,
    pub backups: Vec<Backup>,
    pub online: bool,
    pub running: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentReport {
    pub deployment: Option<String>,
    pub status: String,
    pub message: String,
    pub commit: Option<String>,
    /// The agent's `beacon` version.
    #[serde(default)]
    pub agent: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Missing {
    pub hashes: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JoinRequest {
    pub code: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiError {
    pub error: String,
}
