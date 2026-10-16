//! Body of the tokens the auth service signs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What a token is for. Only [`TokenKind::Access`] grants access to the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    /// Short-lived access token sent as `Authorization: Bearer`.
    #[default]
    Access,
    /// Intermediate token between a correct password and the second factor.
    Mfa,
}

/// Claims of a taut token. Times are Unix seconds, as the JWT spec says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    /// Issuer, the auth service.
    pub iss: String,
    /// User id.
    pub sub: Uuid,
    /// Token id, unique per token (a ULID).
    pub jti: String,
    /// Session this token belongs to; revoking the session kills every token minted for it.
    pub sid: Uuid,
    /// Issued at.
    pub iat: i64,
    /// Expiration.
    pub exp: i64,
    /// When the user last proved who they are (login, second factor or re-authentication).
    /// Sensitive actions require it to be recent.
    pub auth_time: i64,
    /// Whether the session passed a second factor.
    #[serde(default)]
    pub mfa: bool,
    /// `users.token_version` when the session started; a bump revokes the session.
    #[serde(default = "default_version")]
    pub ver: i32,
    /// Token kind; absent means access.
    #[serde(default)]
    pub kind: TokenKind,
}

const fn default_version() -> i32 {
    1
}

impl Claims {
    /// Issue time as a date.
    pub fn issued_at(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.iat, 0).unwrap_or_default()
    }

    /// Last authentication as a date.
    pub fn authenticated_at(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.auth_time, 0).unwrap_or_default()
    }

    /// Seconds since the user last proved who they are.
    pub fn auth_age_secs(&self, now: DateTime<Utc>) -> i64 {
        now.timestamp() - self.auth_time
    }
}
