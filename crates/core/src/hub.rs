//! HTTP client for `beacon-hub`.

use crate::{api::*, settings::Settings};
use anyhow::{Result, anyhow, bail};
use reqwest::{Method, StatusCode};
use serde::{Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Hub {
    pub base: String,
    token: Option<String>,
}

impl Hub {
    pub fn new(base: &str, token: Option<String>) -> Self {
        Self { base: base.trim_end_matches('/').to_string(), token }
    }

    pub fn from_settings(s: &Settings) -> Self {
        Self::new(&s.hub_url(), s.token.clone())
    }

    /// Like [`from_settings`] but fails early when not logged in.
    pub fn authed(s: &Settings) -> Result<Self> {
        if s.token.is_none() {
            bail!("not signed in; sign in to your BeaconSync hub first");
        }
        Ok(Self::from_settings(s))
    }

    fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let mut req = crate::http().request(method, format!("{}{path}", self.base));
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        req
    }

    async fn check(resp: reqwest::Response) -> Result<reqwest::Response> {
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<ApiError>(&text).map(|e| e.error).unwrap_or(text);
        Err(match status {
            StatusCode::UNAUTHORIZED => anyhow!("signed out of the hub, please sign in again"),
            _ if msg.is_empty() => anyhow!("hub returned {status}"),
            _ => anyhow!(msg),
        })
    }

    async fn call<T: DeserializeOwned>(&self, method: Method, path: &str, body: Option<&impl Serialize>) -> Result<T> {
        let mut req = self.request(method, path);
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await.map_err(|e| anyhow!("cannot reach hub {}: {e}", self.base))?;
        Ok(Self::check(resp).await?.json().await?)
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.call(Method::GET, path, None::<&()>).await
    }

    async fn post<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T> {
        self.call(Method::POST, path, Some(body)).await
    }

    // ---- auth ---------------------------------------------------------------------------

    pub async fn register(&self, c: &Credentials) -> Result<AuthResponse> {
        self.post("/api/auth/register", c).await
    }

    pub async fn login(&self, c: &Credentials) -> Result<AuthResponse> {
        self.post("/api/auth/login", c).await
    }

    pub async fn device_start(&self) -> Result<DeviceStart> {
        self.post("/api/auth/device/start", &()).await
    }

    /// `None` while the code is still waiting for approval.
    pub async fn device_poll(&self, device_code: &str) -> Result<Option<AuthResponse>> {
        let resp = self
            .request(Method::POST, "/api/auth/device/poll")
            .json(&DevicePoll { device_code: device_code.into() })
            .send()
            .await?;
        if resp.status() == StatusCode::PRECONDITION_REQUIRED {
            return Ok(None);
        }
        Ok(Some(Self::check(resp).await?.json().await?))
    }

    pub async fn device_approve(&self, user_code: &str) -> Result<serde_json::Value> {
        self.post("/api/auth/device/approve", &DeviceApprove { user_code: user_code.into() }).await
    }

    pub async fn me(&self) -> Result<User> {
        self.get("/api/me").await
    }

    // ---- packs --------------------------------------------------------------------------

    pub async fn library(&self) -> Result<Vec<Pack>> {
        self.get("/api/library").await
    }

    pub async fn join(&self, code: &str) -> Result<Pack> {
        self.post("/api/library/join", &JoinRequest { code: code.into() }).await
    }

    pub async fn create_pack(&self, p: &NewPack) -> Result<Pack> {
        self.post("/api/packs", p).await
    }

    pub async fn pack(&self, id: &str) -> Result<Pack> {
        self.get(&format!("/api/packs/{id}")).await
    }

    pub async fn patch_pack(&self, id: &str, p: &PackPatch) -> Result<Pack> {
        self.call(Method::PATCH, &format!("/api/packs/{id}"), Some(p)).await
    }

    pub async fn delete_pack(&self, id: &str) -> Result<serde_json::Value> {
        self.call(Method::DELETE, &format!("/api/packs/{id}"), None::<&()>).await
    }

    pub async fn refs(&self, pack: &str) -> Result<BTreeMap<String, String>> {
        self.get(&format!("/api/packs/{pack}/refs")).await
    }

    pub async fn update_ref(&self, pack: &str, branch: &str, u: &RefUpdate) -> Result<serde_json::Value> {
        self.call(Method::PUT, &format!("/api/packs/{pack}/refs/{branch}"), Some(u)).await
    }

    pub async fn releases(&self, pack: &str) -> Result<Vec<Release>> {
        self.get(&format!("/api/packs/{pack}/releases")).await
    }

    pub async fn create_release(&self, pack: &str, r: &NewRelease) -> Result<Release> {
        self.post(&format!("/api/packs/{pack}/releases"), r).await
    }

    pub async fn servers(&self, pack: &str) -> Result<Vec<Server>> {
        self.get(&format!("/api/packs/{pack}/servers")).await
    }

    pub async fn create_server(&self, pack: &str, s: &NewServer) -> Result<NewServerResponse> {
        self.post(&format!("/api/packs/{pack}/servers"), s).await
    }

    pub async fn delete_server(&self, pack: &str, server: &str) -> Result<serde_json::Value> {
        self.call(Method::DELETE, &format!("/api/packs/{pack}/servers/{server}"), None::<&()>).await
    }

    pub async fn deploy(&self, pack: &str, d: &DeployRequest) -> Result<DeployResponse> {
        self.post(&format!("/api/packs/{pack}/deploy"), d).await
    }

    pub async fn public_pack(&self, code: &str) -> Result<PublicPack> {
        self.get(&format!("/api/public/packs/{}", code.trim().to_uppercase())).await
    }

    // ---- objects ------------------------------------------------------------------------

    pub async fn get_object(&self, hash: &str) -> Result<Vec<u8>> {
        let resp = self.request(Method::GET, &format!("/api/objects/{hash}")).send().await?;
        Ok(Self::check(resp).await?.bytes().await?.to_vec())
    }

    pub async fn missing(&self, hashes: Vec<String>) -> Result<Vec<String>> {
        let mut out = vec![];
        for chunk in hashes.chunks(500) {
            let m: Missing = self.post("/api/objects/missing", &Missing { hashes: chunk.to_vec() }).await?;
            out.extend(m.hashes);
        }
        Ok(out)
    }

    pub async fn put_object(&self, hash: &str, bytes: Vec<u8>) -> Result<()> {
        let resp = self.request(Method::PUT, &format!("/api/objects/{hash}")).body(bytes).send().await?;
        Self::check(resp).await?;
        Ok(())
    }

    // ---- server agent -------------------------------------------------------------------

    /// Long polls for a deploy target and owner actions. `known` is the deployment the agent
    /// already has, so the hub keeps waiting instead of returning it again.
    pub async fn agent_poll(&self, wait_secs: u64, known: Option<&str>) -> Result<AgentPoll> {
        let known = known.map(|k| format!("&known={k}")).unwrap_or_default();
        let resp = self
            .request(Method::GET, &format!("/api/agent/poll?wait={wait_secs}{known}"))
            .timeout(std::time::Duration::from_secs(wait_secs + 20))
            .send()
            .await?;
        Ok(Self::check(resp).await?.json().await?)
    }

    pub async fn agent_report(&self, r: &AgentReport) -> Result<serde_json::Value> {
        self.post("/api/agent/report", r).await
    }

    pub async fn agent_console(&self, c: &AgentConsole) -> Result<serde_json::Value> {
        self.post("/api/agent/console", c).await
    }

    // ---- server management (owners) -------------------------------------------------------

    /// Console lines after `after` (the last ones when None), waiting up to `wait_secs` for new ones.
    pub async fn console(&self, server: &str, after: Option<u64>, wait_secs: u64) -> Result<ConsoleView> {
        let after = after.map(|a| format!("&after={a}")).unwrap_or_default();
        let resp = self
            .request(Method::GET, &format!("/api/servers/{server}/console?wait={wait_secs}{after}"))
            .timeout(std::time::Duration::from_secs(wait_secs + 20))
            .send()
            .await?;
        Ok(Self::check(resp).await?.json().await?)
    }

    pub async fn server_action(&self, server: &str, a: &ServerAction) -> Result<serde_json::Value> {
        self.post(&format!("/api/servers/{server}/actions"), a).await
    }
}

/// Extracts the share code from a code or a `.../j/CODE` link.
pub fn parse_code(input: &str) -> String {
    let s = input.trim().trim_end_matches('/');
    s.rsplit('/').next().unwrap_or(s).trim().to_uppercase()
}

/// Splits a share link into `(hub, code)` when it is a full URL.
pub fn parse_link(input: &str) -> Option<(String, String)> {
    let s = input.trim().trim_end_matches('/');
    let (hub, code) = s.rsplit_once("/j/")?;
    (hub.starts_with("http://") || hub.starts_with("https://")).then(|| (hub.to_string(), code.to_uppercase()))
}
