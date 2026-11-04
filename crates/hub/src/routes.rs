//! HTTP API. Shapes are defined in `beacon_core::api` and documented in ARCHITECTURE.md.

use crate::{
    Config,
    auth::{AuthServer, AuthUser, hash_password, verify_password},
    db::{self, PACK_COLS, RELEASE_COLS, SERVER_COLS, new_code, new_id, new_token, sha256},
    error::{Error, Result},
    pages,
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post, put},
};
use beacon_core::{api::*, hash, model::Commit, now, store::ObjectStore};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};
use tokio::sync::Notify;

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

pub struct Inner {
    db: Mutex<Connection>,
    store: ObjectStore,
    pub config: Config,
    /// Wakes long polling agents when they get a new deployment.
    waiters: Mutex<HashMap<String, Arc<Notify>>>,
}

impl std::ops::Deref for AppState {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl AppState {
    pub fn open(config: Config) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&config.data)?;
        let db = db::open(&config.data.join("hub.db"))?;
        let store = ObjectStore::new(config.data.join("objects"));
        Ok(Self(Arc::new(Inner { db: Mutex::new(db), store, config, waiters: Mutex::default() })))
    }

    pub fn db(&self) -> MutexGuard<'_, Connection> {
        self.0.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn waiter(&self, server: &str) -> Arc<Notify> {
        self.waiters.lock().unwrap_or_else(|e| e.into_inner()).entry(server.to_string()).or_default().clone()
    }

    pub fn public_url(&self) -> String {
        self.config.public_url.clone().unwrap_or_else(|| format!("http://{}", self.config.addr)).trim_end_matches('/').to_string()
    }
}

pub fn router(state: AppState) -> Router {
    let max = state.config.max_object_mb * 1024 * 1024;
    Router::new()
        .route("/", get(pages::home))
        .route("/j/{code}", get(pages::share))
        .route("/device", get(pages::device_form).post(pages::device_submit))
        .route("/api/health", get(|| async { Json(json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") })) }))
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/device/start", post(device_start))
        .route("/api/auth/device/poll", post(device_poll))
        .route("/api/auth/device/approve", post(device_approve))
        .route("/api/me", get(me))
        .route("/api/library", get(library))
        .route("/api/library/join", post(join))
        .route("/api/packs", post(create_pack))
        .route("/api/packs/{id}", get(get_pack).patch(patch_pack).delete(delete_pack))
        .route("/api/packs/{id}/refs", get(get_refs))
        .route("/api/packs/{id}/refs/{branch}", put(put_ref))
        .route("/api/packs/{id}/releases", get(list_releases).post(create_release))
        .route("/api/packs/{id}/servers", get(list_servers).post(create_server))
        .route("/api/packs/{id}/servers/{sid}", axum::routing::delete(delete_server))
        .route("/api/packs/{id}/deploy", post(deploy))
        .route("/api/public/packs/{code}", get(public_pack))
        .route("/api/objects/missing", post(missing))
        .route("/api/objects/{hash}", get(get_object).put(put_object).layer(DefaultBodyLimit::max(max)))
        .route("/api/agent/poll", get(agent_poll))
        .route("/api/agent/report", post(agent_report))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

// ---- auth -------------------------------------------------------------------------------

fn issue_token(db: &Connection, user: &str, label: &str) -> Result<String> {
    let token = new_token();
    db.execute(
        "insert into tokens (hash, user_id, label, created) values (?1, ?2, ?3, ?4)",
        params![sha256(&token), user, label, now()],
    )?;
    Ok(token)
}

fn valid_username(u: &str) -> bool {
    (3..=32).contains(&u.len()) && u.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

async fn register(State(s): State<AppState>, Json(c): Json<Credentials>) -> Result<Json<AuthResponse>> {
    if !s.config.open_registration {
        return Err(Error(StatusCode::FORBIDDEN, "registration is closed on this hub".into()));
    }
    let username = c.username.trim();
    if !valid_username(username) {
        return Err(Error::bad("username must be 3-32 letters, numbers, _ or -"));
    }
    if c.password.len() < 8 {
        return Err(Error::bad("password must be at least 8 characters"));
    }
    let pw = hash_password(&c.password)?;
    let db = s.db();
    let id = new_id();
    let taken = db.query_row("select 1 from users where username = ?1", params![username], |_| Ok(())).optional()?.is_some();
    if taken {
        return Err(Error::conflict("that username is taken"));
    }
    db.execute("insert into users (id, username, pw_hash, created) values (?1, ?2, ?3, ?4)", params![id, username, pw, now()])?;
    let token = issue_token(&db, &id, "register")?;
    Ok(Json(AuthResponse { token, user: User { id, username: username.into() } }))
}

/// Checks a username/password pair (also used by the /device web page).
pub fn check_credentials(s: &AppState, c: &Credentials) -> Result<User> {
    let db = s.db();
    let row: Option<(String, String, String)> = db
        .query_row("select id, username, pw_hash from users where username = ?1", params![c.username.trim()], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .optional()?;
    match row {
        Some((id, username, hash)) if verify_password(&c.password, &hash) => Ok(User { id, username }),
        _ => Err(Error(StatusCode::UNAUTHORIZED, "wrong username or password".into())),
    }
}

async fn login(State(s): State<AppState>, Json(c): Json<Credentials>) -> Result<Json<AuthResponse>> {
    let user = check_credentials(&s, &c)?;
    let token = issue_token(&s.db(), &user.id, "login")?;
    Ok(Json(AuthResponse { token, user }))
}

async fn device_start(State(s): State<AppState>) -> Result<Json<DeviceStart>> {
    let device_code = new_token();
    let user_code = format!("{}-{}", new_code(4), new_code(4));
    let db = s.db();
    db.execute("delete from devices where expires < ?1", params![now()])?;
    db.execute(
        "insert into devices (device_code, user_code, expires) values (?1, ?2, ?3)",
        params![device_code, user_code, now() + 600],
    )?;
    Ok(Json(DeviceStart { device_code, user_code, expires_in: 600 }))
}

async fn device_poll(State(s): State<AppState>, Json(p): Json<DevicePoll>) -> Result<Json<AuthResponse>> {
    let db = s.db();
    let row: Option<(Option<String>, i64)> = db
        .query_row("select user_id, expires from devices where device_code = ?1", params![p.device_code], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    let (user_id, expires) = row.ok_or_else(|| Error::bad("unknown or expired code"))?;
    if expires < now() {
        return Err(Error(StatusCode::GONE, "the code expired, start again".into()));
    }
    let Some(user_id) = user_id else {
        return Err(Error(StatusCode::PRECONDITION_REQUIRED, "waiting for approval".into()));
    };
    db.execute("delete from devices where device_code = ?1", params![p.device_code])?;
    let username: String = db.query_row("select username from users where id = ?1", params![user_id], |r| r.get(0))?;
    let token = issue_token(&db, &user_id, "device")?;
    Ok(Json(AuthResponse { token, user: User { id: user_id, username } }))
}

pub fn approve_device(s: &AppState, user: &str, code: &str) -> Result<()> {
    let n = s.db().execute(
        "update devices set user_id = ?1 where user_code = ?2 and expires >= ?3 and user_id is null",
        params![user, code.trim().to_uppercase(), now()],
    )?;
    if n == 0 {
        return Err(Error::bad("that code is unknown or expired"));
    }
    Ok(())
}

async fn device_approve(State(s): State<AppState>, u: AuthUser, Json(a): Json<DeviceApprove>) -> Result<Json<Value>> {
    approve_device(&s, &u.id, &a.user_code)?;
    Ok(Json(json!({ "ok": true })))
}

async fn me(u: AuthUser) -> Json<User> {
    Json(User { id: u.id, username: u.username })
}

// ---- packs ------------------------------------------------------------------------------

fn load_pack(db: &Connection, id: &str, viewer: Option<&str>) -> Result<Pack> {
    let mut pack = db
        .query_row(
            &format!("select {PACK_COLS} from packs p join users u on u.id = p.owner_id where p.id = ?1"),
            params![id],
            |r| db::pack_row(r, viewer),
        )
        .optional()?
        .ok_or_else(|| Error::not_found("pack"))?;
    pack.latest = db::latest_release(db, id)?;
    Ok(pack)
}

fn owner_of(db: &Connection, pack: &str) -> Result<String> {
    db.query_row("select owner_id from packs where id = ?1", params![pack], |r| r.get(0))
        .optional()?
        .ok_or_else(|| Error::not_found("pack"))
}

fn require_owner(db: &Connection, pack: &str, user: &AuthUser) -> Result<()> {
    if owner_of(db, pack)? == user.id { Ok(()) } else { Err(Error::forbidden()) }
}

fn require_read(db: &Connection, pack: &str, user: &AuthUser) -> Result<()> {
    if owner_of(db, pack)? == user.id {
        return Ok(());
    }
    let member = db
        .query_row("select 1 from library where user_id = ?1 and pack_id = ?2", params![user.id, pack], |_| Ok(()))
        .optional()?;
    member.ok_or_else(Error::forbidden)
}

async fn library(State(s): State<AppState>, u: AuthUser) -> Result<Json<Vec<Pack>>> {
    let db = s.db();
    let ids: Vec<String> = db
        .prepare("select id from packs where owner_id = ?1 union select pack_id from library where user_id = ?1")?
        .query_map(params![u.id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut packs = ids.iter().map(|id| load_pack(&db, id, Some(&u.id))).collect::<Result<Vec<_>>>()?;
    packs.sort_by_key(|p| std::cmp::Reverse(p.updated));
    Ok(Json(packs))
}

async fn join(State(s): State<AppState>, u: AuthUser, Json(j): Json<JoinRequest>) -> Result<Json<Pack>> {
    let db = s.db();
    let code = beacon_core::hub::parse_code(&j.code);
    let id: String = db
        .query_row("select id from packs where code = ?1", params![code], |r| r.get(0))
        .optional()?
        .ok_or_else(|| Error::not_found("pack"))?;
    db.execute("insert or ignore into library (user_id, pack_id, created) values (?1, ?2, ?3)", params![u.id, id, now()])?;
    Ok(Json(load_pack(&db, &id, Some(&u.id))?))
}

async fn create_pack(State(s): State<AppState>, u: AuthUser, Json(p): Json<NewPack>) -> Result<Json<Pack>> {
    let name = p.name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(Error::bad("pack name must be 1-80 characters"));
    }
    let db = s.db();
    let id = new_id();
    let loader = serde_json::to_string(&p.loader).map_err(anyhow::Error::from)?;
    // Codes are short, retry on the rare collision.
    for _ in 0..8 {
        let code = new_code(6);
        let res = db.execute(
            "insert into packs (id, owner_id, code, name, description, minecraft, loader, created, updated) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![id, u.id, code, name, p.description, p.minecraft, loader, now()],
        );
        match res {
            Ok(_) => return Ok(Json(load_pack(&db, &id, Some(&u.id))?)),
            Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(Error::conflict("could not allocate a share code, try again"))
}

async fn get_pack(State(s): State<AppState>, u: AuthUser, Path(id): Path<String>) -> Result<Json<Pack>> {
    let db = s.db();
    require_read(&db, &id, &u)?;
    Ok(Json(load_pack(&db, &id, Some(&u.id))?))
}

async fn patch_pack(
    State(s): State<AppState>,
    u: AuthUser,
    Path(id): Path<String>,
    Json(p): Json<PackPatch>,
) -> Result<Json<Pack>> {
    let db = s.db();
    require_owner(&db, &id, &u)?;
    if let Some(n) = p.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        db.execute("update packs set name = ?1 where id = ?2", params![n, id])?;
    }
    if let Some(d) = &p.description {
        db.execute("update packs set description = ?1 where id = ?2", params![d, id])?;
    }
    if let Some(mc) = &p.minecraft {
        db.execute("update packs set minecraft = ?1 where id = ?2", params![mc, id])?;
    }
    if let Some(l) = &p.loader {
        db.execute(
            "update packs set loader = ?1 where id = ?2",
            params![serde_json::to_string(l).map_err(anyhow::Error::from)?, id],
        )?;
    }
    db.execute("update packs set updated = ?1 where id = ?2", params![now(), id])?;
    Ok(Json(load_pack(&db, &id, Some(&u.id))?))
}

async fn delete_pack(State(s): State<AppState>, u: AuthUser, Path(id): Path<String>) -> Result<Json<Value>> {
    let db = s.db();
    require_owner(&db, &id, &u)?;
    db.execute("delete from packs where id = ?1", params![id])?;
    Ok(Json(json!({ "ok": true })))
}

// ---- refs + releases --------------------------------------------------------------------

async fn get_refs(State(s): State<AppState>, u: AuthUser, Path(id): Path<String>) -> Result<Json<HashMap<String, String>>> {
    let db = s.db();
    require_read(&db, &id, &u)?;
    let refs = db
        .prepare("select name, commit_id from refs where pack_id = ?1")?
        .query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Json(refs))
}

fn is_commit(s: &AppState, id: &str) -> bool {
    s.store.get_json::<Commit>(id).is_ok()
}

async fn put_ref(
    State(s): State<AppState>,
    u: AuthUser,
    Path((id, branch)): Path<(String, String)>,
    Json(r): Json<RefUpdate>,
) -> Result<Json<Value>> {
    if branch.is_empty() || branch.len() > 64 || branch.contains("..") {
        return Err(Error::bad("invalid branch name"));
    }
    if !is_commit(&s, &r.new) {
        return Err(Error::bad("upload the commit before moving the branch"));
    }
    let db = s.db();
    require_owner(&db, &id, &u)?;
    let current: Option<String> = db
        .query_row("select commit_id from refs where pack_id = ?1 and name = ?2", params![id, branch], |r| r.get(0))
        .optional()?;
    if !r.force && current != r.old {
        return Err(Error::conflict("the branch moved on the hub; pull first"));
    }
    db.execute(
        "insert into refs (pack_id, name, commit_id) values (?1, ?2, ?3) on conflict(pack_id, name) do update set commit_id = excluded.commit_id",
        params![id, branch, r.new],
    )?;
    db.execute("update packs set updated = ?1 where id = ?2", params![now(), id])?;
    Ok(Json(json!({ "ok": true })))
}

async fn list_releases(State(s): State<AppState>, u: AuthUser, Path(id): Path<String>) -> Result<Json<Vec<Release>>> {
    let db = s.db();
    require_read(&db, &id, &u)?;
    let rows = db
        .prepare(&format!("select {RELEASE_COLS} from releases where pack_id = ?1 order by created desc, rowid desc"))?
        .query_map(params![id], db::release_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Json(rows))
}

async fn create_release(
    State(s): State<AppState>,
    u: AuthUser,
    Path(id): Path<String>,
    Json(r): Json<NewRelease>,
) -> Result<Json<Release>> {
    if !is_commit(&s, &r.commit) {
        return Err(Error::bad("push the version before releasing it"));
    }
    let name = r.name.trim();
    if name.is_empty() || name.len() > 64 {
        return Err(Error::bad("release name must be 1-64 characters"));
    }
    let db = s.db();
    require_owner(&db, &id, &u)?;
    let release = Release { id: new_id(), commit: r.commit, name: name.into(), notes: r.notes, created: now() };
    db.execute(
        "insert into releases (id, pack_id, commit_id, name, notes, created) values (?1, ?2, ?3, ?4, ?5, ?6)",
        params![release.id, id, release.commit, release.name, release.notes, release.created],
    )?;
    db.execute("update packs set updated = ?1 where id = ?2", params![now(), id])?;
    Ok(Json(release))
}

// ---- servers + deploys ------------------------------------------------------------------

fn servers_of(db: &Connection, pack: &str) -> Result<Vec<Server>> {
    Ok(db
        .prepare(&format!("select {SERVER_COLS} from servers where pack_id = ?1 order by created"))?
        .query_map(params![pack], db::server_row)?
        .collect::<rusqlite::Result<_>>()?)
}

async fn list_servers(State(s): State<AppState>, u: AuthUser, Path(id): Path<String>) -> Result<Json<Vec<Server>>> {
    let db = s.db();
    require_owner(&db, &id, &u)?;
    Ok(Json(servers_of(&db, &id)?))
}

async fn create_server(
    State(s): State<AppState>,
    u: AuthUser,
    Path(id): Path<String>,
    Json(n): Json<NewServer>,
) -> Result<Json<NewServerResponse>> {
    let db = s.db();
    require_owner(&db, &id, &u)?;
    let sid = new_id();
    let token = new_token();
    let mode = serde_json::to_value(n.mode).map_err(anyhow::Error::from)?;
    db.execute(
        "insert into servers (id, pack_id, name, mode, address, token_hash, created) values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![sid, id, n.name.trim(), mode.as_str(), n.address.trim(), sha256(&token), now()],
    )?;
    let server = db.query_row(&format!("select {SERVER_COLS} from servers where id = ?1"), params![sid], db::server_row)?;
    Ok(Json(NewServerResponse { server, token }))
}

async fn delete_server(State(s): State<AppState>, u: AuthUser, Path((id, sid)): Path<(String, String)>) -> Result<Json<Value>> {
    let db = s.db();
    require_owner(&db, &id, &u)?;
    db.execute("delete from servers where id = ?1 and pack_id = ?2", params![sid, id])?;
    Ok(Json(json!({ "ok": true })))
}

async fn deploy(
    State(s): State<AppState>,
    u: AuthUser,
    Path(id): Path<String>,
    Json(d): Json<DeployRequest>,
) -> Result<Json<DeployResponse>> {
    let (deployment, servers) = {
        let db = s.db();
        require_owner(&db, &id, &u)?;
        let commit: String = db
            .query_row("select commit_id from releases where id = ?1 and pack_id = ?2", params![d.release, id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| Error::not_found("release"))?;
        let all: Vec<String> = servers_of(&db, &id)?.into_iter().map(|s| s.id).collect();
        let targets: Vec<String> = match d.servers {
            DeployTargets::All(_) => all,
            DeployTargets::Some(ids) => ids.into_iter().filter(|i| all.contains(i)).collect(),
        };
        if targets.is_empty() {
            return Err(Error::bad("no servers selected"));
        }
        let deployment = new_id();
        db.execute(
            "insert into deployments (id, pack_id, release_id, created_by, created) values (?1, ?2, ?3, ?4, ?5)",
            params![deployment, id, d.release, u.id, now()],
        )?;
        for t in &targets {
            db.execute(
                "update servers set target_commit = ?1, target_release = ?2, deployment = ?3, status = 'pending', message = 'waiting for server' where id = ?4",
                params![commit, d.release, deployment, t],
            )?;
        }
        (deployment, targets)
    };
    for sid in &servers {
        s.waiter(sid).notify_waiters();
    }
    Ok(Json(DeployResponse { deployment, servers }))
}

// ---- public -----------------------------------------------------------------------------

pub fn public_info(s: &AppState, code: &str) -> Result<PublicPack> {
    let db = s.db();
    let code = beacon_core::hub::parse_code(code);
    let id: String = db
        .query_row("select id from packs where code = ?1", params![code], |r| r.get(0))
        .optional()?
        .ok_or_else(|| Error::not_found("pack"))?;
    let p = load_pack(&db, &id, None)?;
    let servers = servers_of(&db, &id)?
        .into_iter()
        .map(|s| PublicServer { name: s.name, address: s.address, commit: s.current_commit, online: s.online })
        .collect();
    Ok(PublicPack {
        id: p.id,
        code: p.code,
        name: p.name,
        owner: p.owner,
        description: p.description,
        minecraft: p.minecraft,
        loader: p.loader,
        latest: p.latest,
        servers,
    })
}

async fn public_pack(State(s): State<AppState>, Path(code): Path<String>) -> Result<Json<PublicPack>> {
    Ok(Json(public_info(&s, &code)?))
}

// ---- objects ----------------------------------------------------------------------------

async fn missing(State(s): State<AppState>, _u: AuthUser, Json(m): Json<Missing>) -> Result<Json<Missing>> {
    if m.hashes.len() > 1000 {
        return Err(Error::bad("ask for at most 1000 objects at a time"));
    }
    Ok(Json(Missing { hashes: m.hashes.into_iter().filter(|h| hash::is_hash(h) && !s.store.has(h)).collect() }))
}

async fn put_object(State(s): State<AppState>, _u: AuthUser, Path(id): Path<String>, body: Bytes) -> Result<StatusCode> {
    if !hash::is_hash(&id) {
        return Err(Error::bad("invalid object id"));
    }
    let store = s.store.clone();
    tokio::task::spawn_blocking(move || store.put_verified(&id, &body))
        .await
        .map_err(anyhow::Error::from)?
        .map_err(|e| Error::bad(format!("{e:#}")))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_object(State(s): State<AppState>, Path(id): Path<String>) -> Result<impl IntoResponse> {
    if !hash::is_hash(&id) {
        return Err(Error::bad("invalid object id"));
    }
    let bytes = tokio::fs::read(s.store.path(&id)).await.map_err(|_| Error::not_found("object"))?;
    Ok((
        [(header::CONTENT_TYPE, "application/octet-stream"), (header::CACHE_CONTROL, "public, max-age=31536000, immutable")],
        bytes,
    ))
}

// ---- agents -----------------------------------------------------------------------------

#[derive(Deserialize)]
struct PollQuery {
    wait: Option<u64>,
}

fn agent_state(s: &AppState, srv: &AuthServer) -> Result<(AgentPoll, bool)> {
    let db = s.db();
    db.execute("update servers set last_seen = ?1 where id = ?2", params![now(), srv.id])?;
    let server = db.query_row(&format!("select {SERVER_COLS} from servers where id = ?1"), params![srv.id], db::server_row)?;
    let (deployment, failed): (Option<String>, Option<String>) =
        db.query_row("select deployment, failed_deployment from servers where id = ?1", params![srv.id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    let code: String = db.query_row("select code from packs where id = ?1", params![srv.pack_id], |r| r.get(0))?;
    // A deployment the server failed or rolled back is not offered again; a new deploy is.
    let pending = server.target_commit.is_some() && server.target_commit != server.current_commit && deployment != failed;
    let target = match (pending, &server.target_commit, &server.target_release) {
        (true, Some(commit), Some(release)) => {
            let release_name: String = db
                .query_row("select name from releases where id = ?1", params![release], |r| r.get(0))
                .optional()?
                .unwrap_or_default();
            Some(AgentTarget {
                commit: commit.clone(),
                release: release.clone(),
                release_name,
                deployment: deployment.unwrap_or_default(),
            })
        }
        _ => None,
    };
    let has = target.is_some();
    Ok((AgentPoll { target, server, pack_code: code }, has))
}

async fn agent_poll(State(s): State<AppState>, srv: AuthServer, Query(q): Query<PollQuery>) -> Result<Json<AgentPoll>> {
    let wait = Duration::from_secs(q.wait.unwrap_or(25).min(30));
    let deadline = tokio::time::Instant::now() + wait;
    let notify = s.waiter(&srv.id);
    loop {
        let notified = notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let (poll, has_target) = agent_state(&s, &srv)?;
        if has_target || tokio::time::Instant::now() >= deadline {
            return Ok(Json(poll));
        }
        tokio::select! {
            _ = &mut notified => {}
            _ = tokio::time::sleep_until(deadline) => {}
        }
    }
}

async fn agent_report(State(s): State<AppState>, srv: AuthServer, Json(r): Json<AgentReport>) -> Result<Json<Value>> {
    if !["idle", "applying", "done", "failed", "running", "stopped", "rolledback"].contains(&r.status.as_str()) {
        return Err(Error::bad("unknown status"));
    }
    let db = s.db();
    let message: String = r.message.chars().take(500).collect();
    db.execute(
        "update servers set status = ?1, message = ?2, last_seen = ?3 where id = ?4",
        params![r.status, message, now(), srv.id],
    )?;
    match (r.status.as_str(), &r.deployment) {
        ("failed", Some(d)) => {
            db.execute("update servers set failed_deployment = ?1 where id = ?2", params![d, srv.id])?;
        }
        ("rolledback", _) => {
            db.execute("update servers set failed_deployment = deployment where id = ?1", params![srv.id])?;
        }
        _ => {}
    }
    if let Some(c) = r.commit.filter(|c| hash::is_hash(c)) {
        db.execute("update servers set current_commit = ?1 where id = ?2", params![c, srv.id])?;
    }
    Ok(Json(json!({ "ok": true })))
}
