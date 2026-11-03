//! Bearer token extractors for users and server agents.

use crate::{
    db::sha256,
    error::{Error, Result},
    routes::AppState,
};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{extract::FromRequestParts, http::request::Parts};
use rusqlite::{OptionalExtension, params};

/// A signed in user.
pub struct AuthUser {
    pub id: String,
    pub username: String,
}

/// A game server agent, authenticated with its server token.
pub struct AuthServer {
    pub id: String,
    pub pack_id: String,
}

fn bearer(parts: &Parts) -> Option<&str> {
    parts.headers.get("authorization")?.to_str().ok()?.strip_prefix("Bearer ").map(str::trim)
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self> {
        let token = bearer(parts).ok_or_else(Error::unauthorized)?;
        let db = state.db();
        db.query_row(
            "select u.id, u.username from tokens t join users u on u.id = t.user_id where t.hash = ?1",
            params![sha256(token)],
            |r| Ok(AuthUser { id: r.get(0)?, username: r.get(1)? }),
        )
        .optional()?
        .ok_or_else(Error::unauthorized)
    }
}

impl FromRequestParts<AppState> for AuthServer {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self> {
        let token = bearer(parts).ok_or_else(Error::unauthorized)?;
        let db = state.db();
        db.query_row("select id, pack_id from servers where token_hash = ?1", params![sha256(token)], |r| {
            Ok(AuthServer { id: r.get(0)?, pack_id: r.get(1)? })
        })
        .optional()?
        .ok_or_else(Error::unauthorized)
    }
}

pub fn hash_password(pw: &str) -> Result<String> {
    Argon2::default()
        .hash_password(pw.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|h| h.to_string())
        .map_err(|e| Error::bad(format!("cannot hash password: {e}")))
}

pub fn verify_password(pw: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok())
}
