use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use beacon_core::api::ApiError;

/// Error returned by handlers, rendered as `{ "error": "..." }`.
#[derive(Debug)]
pub struct Error(pub StatusCode, pub String);

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn bad(msg: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, msg.into())
    }
    pub fn not_found(what: &str) -> Self {
        Self(StatusCode::NOT_FOUND, format!("{what} not found"))
    }
    pub fn forbidden() -> Self {
        Self(StatusCode::FORBIDDEN, "you don't have access to this pack".into())
    }
    pub fn unauthorized() -> Self {
        Self(StatusCode::UNAUTHORIZED, "sign in required".into())
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self(StatusCode::CONFLICT, msg.into())
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        if self.0.is_server_error() {
            tracing::error!("{}", self.1);
        }
        (self.0, Json(ApiError { error: self.1 })).into_response()
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, format!("database error: {e}"))
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"))
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, format!("io error: {e}"))
    }
}
