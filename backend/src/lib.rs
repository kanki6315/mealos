pub mod apple;
pub mod assistant;
pub mod auth;
pub mod db;
pub mod http;
pub mod library_sync;
pub mod recipes;

use axum::{
    Json,
    extract::rejection::JsonRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug)]
pub struct Error(pub StatusCode, pub &'static str);

impl Error {
    pub fn unauthorized() -> Self {
        Self(StatusCode::UNAUTHORIZED, "Sign in with Apple again.")
    }
    pub fn unavailable() -> Self {
        Self(
            StatusCode::SERVICE_UNAVAILABLE,
            "Service temporarily unavailable.",
        )
    }
    pub fn bad_request() -> Self {
        Self(StatusCode::BAD_REQUEST, "Invalid request.")
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}

impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(code = ?error.as_database_error().and_then(|e| e.code()), "Database operation failed");
        Self::unavailable()
    }
}

impl From<JsonRejection> for Error {
    fn from(error: JsonRejection) -> Self {
        if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
            Error(StatusCode::PAYLOAD_TOO_LARGE, "Request is too large.")
        } else {
            Error::bad_request()
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.1)
    }
}

impl std::error::Error for Error {}
