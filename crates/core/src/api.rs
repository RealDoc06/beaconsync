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
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentReport {
    pub deployment: Option<String>,
    pub status: String,
    pub message: String,
    pub commit: Option<String>,
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
