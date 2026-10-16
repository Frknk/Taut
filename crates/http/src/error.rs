//! Error type returned by all handlers.
//!
//! Every error becomes a JSON body `{"error": {"code", "message", "trace_id"}}` with the
//! matching status code. Internal errors are logged with their cause and answered with a
//! generic message, so nothing about the implementation leaks to clients.

use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use taut_db::DbError;

use crate::layers::current_trace_id;

/// Error returned by handlers.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The requested resource does not exist.
    #[error("{0}")]
    NotFound(String),
    /// The request is malformed or fails validation.
    #[error("{0}")]
    BadRequest(String),
    /// Authentication is missing or invalid.
    #[error("{0}")]
    Unauthorized(String),
    /// The caller is authenticated but not allowed to do this.
    #[error("{0}")]
    Forbidden(String),
    /// The request conflicts with existing data, for example a duplicate unique value.
    #[error("{0}")]
    Conflict(String),
    /// A dependency (database, bus, cache) is unavailable.
    #[error("{0}")]
    Unavailable(String),
    /// Unexpected failure. The cause is logged, never sent to the client.
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    /// HTTP status code for this error.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Stable machine-readable code for this error.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::BadRequest(_) => "bad_request",
            Self::Unauthorized(_) => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::Conflict(_) => "conflict",
            Self::Unavailable(_) => "unavailable",
            Self::Internal(_) => "internal",
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        Self::Internal(err.into())
    }
}

impl From<JsonRejection> for AppError {
    fn from(rejection: JsonRejection) -> Self {
        Self::BadRequest(rejection.body_text())
    }
}

impl From<PathRejection> for AppError {
    fn from(rejection: PathRejection) -> Self {
        Self::BadRequest(rejection.body_text())
    }
}

impl From<QueryRejection> for AppError {
    fn from(rejection: QueryRejection) -> Self {
        Self::BadRequest(rejection.body_text())
    }
}

impl From<DbError> for AppError {
    fn from(err: DbError) -> Self {
        match err {
            DbError::Connect(_) | DbError::Ping(_) => {
                Self::Unavailable("database unavailable".to_owned())
            }
            DbError::Migrate(_) => Self::Internal(err.into()),
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_id: Option<String>,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        if status.is_server_error() {
            tracing::error!(error = ?self, "request failed");
        }
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code(),
                message: self.to_string(),
                trace_id: current_trace_id(),
            },
        };
        (status, Json(body)).into_response()
    }
}
