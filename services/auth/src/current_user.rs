//! The user behind a bearer token, for the routes that need a session (`me`, password change).

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::Utc;
use taut_http::AppError;
use taut_identity::Claims;
use taut_identity::bearer::{self, Credential};

use crate::AppState;
use crate::users::{self, User};

/// A verified access token and the user it names. Rejects with `401` when the token is
/// missing, invalid, expired, not an access token, or from a session ended by a password
/// change (its `ver` is behind `users.token_version`).
#[derive(Debug, Clone)]
pub struct CurrentUser {
    /// Claims of the token.
    pub claims: Claims,
    /// The user row.
    pub user: User,
}

impl CurrentUser {
    /// Fails with `403 reauth_required` unless the user confirmed their password within the
    /// last five minutes (`POST /v1/auth/reauth`). The sensitive routes call it first.
    pub fn require_recent_auth(&self) -> Result<(), AppError> {
        if self.claims.is_recent(Utc::now()) {
            Ok(())
        } else {
            Err(AppError::Denied {
                code: "reauth_required",
                message: "confirm your password to continue".to_owned(),
            })
        }
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let token = match bearer::credential(&parts.headers) {
            Some(Credential::Jwt(token)) => token,
            Some(Credential::ApiKey(_)) => {
                return Err(AppError::Unauthorized(
                    "a user session is required, not an api key".to_owned(),
                ));
            }
            None => return Err(AppError::Unauthorized("missing bearer token".to_owned())),
        };
        let claims = state
            .verifier
            .verify_access(token)
            .await
            .map_err(|err| AppError::Unauthorized(err.to_string()))?;
        let user = users::find_by_id(&state.pool, claims.sub)
            .await?
            .filter(|user| user.token_version == claims.ver)
            .ok_or_else(|| AppError::Unauthorized("session ended".to_owned()))?;
        Ok(Self { claims, user })
    }
}
