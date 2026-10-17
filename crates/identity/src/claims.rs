//! Body of the tokens the auth service signs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Seconds since the last authentication after which sensitive actions (deleting a tenant,
/// creating API keys, changing alert channels, disabling the second factor, changing the
/// password) demand that the user confirms their password again.
pub const RECENT_AUTH_SECS: i64 = 300;

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

/// Claims of a Taut token. Times are Unix seconds, as the JWT spec says.
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

    /// Whether the user proved who they are within [`RECENT_AUTH_SECS`]: what the sensitive
    /// actions require (step-up authentication).
    pub fn is_recent(&self, now: DateTime<Utc>) -> bool {
        self.auth_age_secs(now) <= RECENT_AUTH_SECS
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn claims(auth_time: i64) -> Claims {
        Claims {
            iss: "taut-auth".to_owned(),
            sub: Uuid::nil(),
            jti: "01J".to_owned(),
            sid: Uuid::nil(),
            iat: auth_time,
            exp: auth_time + 900,
            auth_time,
            mfa: false,
            ver: 1,
            kind: TokenKind::Access,
        }
    }

    #[test]
    fn recent_means_within_five_minutes() {
        let now = Utc::now();
        assert!(claims(now.timestamp()).is_recent(now));
        assert!(claims(now.timestamp() - RECENT_AUTH_SECS).is_recent(now));
        assert!(!claims(now.timestamp() - RECENT_AUTH_SECS - 1).is_recent(now));
    }
}
