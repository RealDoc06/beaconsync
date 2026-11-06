//! SQLite schema and row mapping.

use beacon_core::{
    api::{Pack, Release, Server, ServerMode},
    model::Loader,
};
use rand::Rng;
use rusqlite::{Connection, OptionalExtension, Row, params};

pub const SCHEMA: &str = r#"
pragma journal_mode = wal;
pragma foreign_keys = on;
create table if not exists users (
  id text primary key,
  username text not null unique collate nocase,
  pw_hash text not null,
  created integer not null
);
create table if not exists tokens (
  hash text primary key,
  user_id text not null references users(id) on delete cascade,
  label text not null,
  created integer not null
);
create table if not exists devices (
  device_code text primary key,
  user_code text not null unique,
  user_id text,
  expires integer not null
);
create table if not exists packs (
  id text primary key,
  owner_id text not null references users(id) on delete cascade,
  code text not null unique,
  name text not null,
  description text not null default '',
  minecraft text not null,
  loader text not null,
  created integer not null,
  updated integer not null
);
create table if not exists refs (
  pack_id text not null references packs(id) on delete cascade,
  name text not null,
  commit_id text not null,
  primary key (pack_id, name)
);
create table if not exists releases (
  id text primary key,
  pack_id text not null references packs(id) on delete cascade,
  commit_id text not null,
  name text not null,
  notes text not null,
  created integer not null
);
create index if not exists releases_pack on releases(pack_id, created);
create table if not exists servers (
  id text primary key,
  pack_id text not null references packs(id) on delete cascade,
  name text not null,
  mode text not null,
  address text not null,
  token_hash text not null unique,
  current_commit text,
  target_commit text,
  target_release text,
  deployment text,
  status text not null default 'idle',
  message text not null default '',
  last_seen integer,
  created integer not null
);
create table if not exists deployments (
  id text primary key,
  pack_id text not null references packs(id) on delete cascade,
  release_id text not null,
  created_by text not null,
  created integer not null
);
create table if not exists library (
  user_id text not null references users(id) on delete cascade,
  pack_id text not null references packs(id) on delete cascade,
  created integer not null,
  primary key (user_id, pack_id)
);
"#;

/// Columns added after the first release; applied when missing.
const MIGRATIONS: &[&str] =
    &["alter table servers add column failed_deployment text", "alter table servers add column agent_version text"];

pub fn open(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    for m in MIGRATIONS {
        match conn.execute(m, []) {
            Err(rusqlite::Error::SqliteFailure(_, Some(msg))) if msg.contains("duplicate column") => {}
            other => {
                other?;
            }
        }
    }
    Ok(conn)
}

/// Random lowercase hex id.
pub fn new_id() -> String {
    hex::encode(rand::thread_rng().r#gen::<[u8; 12]>())
}

/// Random secret token (64 hex chars).
pub fn new_token() -> String {
    hex::encode(rand::thread_rng().r#gen::<[u8; 32]>())
}

/// Human friendly code without look-alike characters.
pub fn new_code(len: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..len).map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char).collect()
}

pub fn sha256(s: &str) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(s.as_bytes()))
}

/// Servers are online when they polled within this many seconds.
pub const ONLINE_SECS: i64 = 90;

pub const SERVER_COLS: &str =
    "id, name, mode, address, current_commit, target_commit, target_release, status, message, last_seen, agent_version";

pub fn server_row(r: &Row) -> rusqlite::Result<Server> {
    let mode: String = r.get(2)?;
    let last_seen: Option<i64> = r.get(9)?;
    let online = last_seen.is_some_and(|t| beacon_core::now() - t < ONLINE_SECS);
    let status: String = r.get(7)?;
    Ok(Server {
        id: r.get(0)?,
        name: r.get(1)?,
        mode: serde_json::from_value(serde_json::Value::String(mode)).unwrap_or(ServerMode::Plain),
        address: r.get(3)?,
        current_commit: r.get(4)?,
        target_commit: r.get(5)?,
        target_release: r.get(6)?,
        status: if online || status == "failed" { status } else { "offline".into() },
        message: r.get(8)?,
        last_seen,
        online,
        agent_version: r.get(10)?,
    })
}

pub const RELEASE_COLS: &str = "id, commit_id, name, notes, created";

pub fn release_row(r: &Row) -> rusqlite::Result<Release> {
    Ok(Release { id: r.get(0)?, commit: r.get(1)?, name: r.get(2)?, notes: r.get(3)?, created: r.get(4)? })
}

pub fn latest_release(db: &Connection, pack: &str) -> rusqlite::Result<Option<Release>> {
    db.query_row(
        &format!("select {RELEASE_COLS} from releases where pack_id = ?1 order by created desc, rowid desc limit 1"),
        params![pack],
        release_row,
    )
    .optional()
}

pub const PACK_COLS: &str =
    "p.id, p.code, p.name, p.description, u.username, p.minecraft, p.loader, p.created, p.updated, p.owner_id";

/// Maps a pack row; `viewer` decides the `mine` flag.
pub fn pack_row(r: &Row, viewer: Option<&str>) -> rusqlite::Result<Pack> {
    let loader: String = r.get(6)?;
    let owner_id: String = r.get(9)?;
    Ok(Pack {
        id: r.get(0)?,
        code: r.get(1)?,
        name: r.get(2)?,
        description: r.get(3)?,
        owner: r.get(4)?,
        minecraft: r.get(5)?,
        loader: serde_json::from_str::<Loader>(&loader).unwrap_or_default(),
        created: r.get(7)?,
        updated: r.get(8)?,
        mine: viewer == Some(owner_id.as_str()),
        latest: None,
    })
}
