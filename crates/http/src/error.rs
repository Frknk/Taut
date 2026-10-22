//! Error type returned by all handlers.
//!
//! Every error becomes a JSON body `{"error": {"code", "message", "trace_id"}}` with the
//! matching status code. Internal errors are logged with their cause and answered with a
//! generic message, so nothing about the implementation leaks to clients. Passing overload (a
//! saturated database pool, a dependency that is down) is a `503` with `Retry-After`, not a
//! `500`: clients may try again, and the logs tell saturation apart from bugs.

use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::http::header::RETRY_AFTER;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use taut_db::DbError;

use crate::layers::current_trace_id;

/// What a `503` asks clients to wait before retrying: enough for a saturated pool to drain a
/// few requests, short enough that a panel poll barely notices.
const UNAVAILABLE_RETRY_AFTER_SECS: u64 = 2;

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
    /// Like [`AppError::Forbidden`], with a stable code the panel reacts to
    /// (`email_unverified`, `not_a_member`, `reauth_required`).
    #[error("{message}")]
    Denied {
        /// Machine-readable reason, in `snake_case`.
        code: &'static str,
        /// Human-readable detail.
        message: String,
    },
    /// Too many attempts; `Retry-After` says how long to wait.
    #[error("too many attempts, retry in {retry_after_secs} s")]
    TooManyRequests {
        /// Seconds until the limit resets.
        retry_after_secs: u64,
    },
    /// The request conflicts with existing data, for example a duplicate unique value.
    #[error("{0}")]
    Conflict(String),
    /// A dependency (database, bus, cache) is unavailable or saturated; `Retry-After` asks the
    /// client to come back in a moment.
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
            Self::Forbidden(_) | Self::Denied { .. } => StatusCode::FORBIDDEN,
            Self::TooManyRequests { .. } => StatusCode::TOO_MANY_REQUESTS,
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
            Self::Denied { code, .. } => code,
            Self::TooManyRequests { .. } => "too_many_requests",
            Self::Conflict(_) => "conflict",
            Self::Unavailable(_) => "unavailable",
            Self::Internal(_) => "internal",
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            // Every connection is busy (or the pool is shutting down): overload, not a bug.
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => {
                Self::Unavailable("database busy, retry shortly".to_owned())
            }
            other => Self::Internal(other.into()),
        }
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
        match &self {
            Self::Internal(_) => tracing::error!(error = ?self, "request failed"),
            Self::Unavailable(_) => tracing::warn!(error = %self, "dependency unavailable"),
            _ => {}
        }
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code(),
                message: self.to_string(),
                trace_id: current_trace_id(),
            },
        };
        let mut response = (status, Json(body)).into_response();
        let retry_after_secs = match self {
            Self::TooManyRequests { retry_after_secs } => Some(retry_after_secs),
            Self::Unavailable(_) => Some(UNAVAILABLE_RETRY_AFTER_SECS),
            _ => None,
        };
        if let Some(secs) = retry_after_secs {
            response
                .headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from(secs));
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;

    use super::*;

    async fn answer(err: AppError) -> (StatusCode, Option<String>, String) {
        let response = err.into_response();
        let status = response.status();
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .map(|value| value.to_str().unwrap().to_owned());
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            retry_after,
            String::from_utf8(body.to_vec()).unwrap(),
        )
    }

    #[tokio::test]
    async fn a_saturated_pool_is_a_503_that_asks_to_retry() {
        for err in [sqlx::Error::PoolTimedOut, sqlx::Error::PoolClosed] {
            let (status, retry_after, body) = answer(err.into()).await;
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(retry_after.as_deref(), Some("2"));
            assert!(body.contains(r#""code":"unavailable""#), "{body}");
            assert!(
                !body.contains("pool"),
                "nothing about the pool leaks: {body}"
            );
        }
    }

    #[tokio::test]
    async fn other_database_errors_stay_internal_and_generic() {
        let (status, retry_after, body) = answer(sqlx::Error::RowNotFound.into()).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(retry_after, None);
        assert!(body.contains(r#""code":"internal""#), "{body}");
        assert!(body.contains(r#""message":"internal error""#), "{body}");
    }

    #[tokio::test]
    async fn too_many_requests_keeps_its_own_retry_after() {
        let too_many = AppError::TooManyRequests {
            retry_after_secs: 17,
        };
        let (status, retry_after, _) = answer(too_many).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(retry_after.as_deref(), Some("17"));
    }
}
