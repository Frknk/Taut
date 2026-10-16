//! One-time tokens sent by email: verification links and password resets.
//!
//! The secret travels in the link; the table keeps its SHA-256. Consuming is a single
//! `UPDATE … RETURNING`, so two clicks on the same link race safely and only one wins.

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use taut_identity::random::{opaque_token, sha256_hex};
use uuid::Uuid;

/// What a token proves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// The user owns the address (24 h).
    VerifyEmail,
    /// The user may choose a new password (1 h).
    ResetPassword,
}

impl TokenKind {
    /// Lowercase name, as stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VerifyEmail => "verify_email",
            Self::ResetPassword => "reset_password",
        }
    }

    /// How long a token of this kind works.
    pub fn ttl(self) -> Duration {
        match self {
            Self::VerifyEmail => Duration::hours(24),
            Self::ResetPassword => Duration::hours(1),
        }
    }
}

/// A freshly issued token.
#[derive(Debug, Clone)]
pub struct Issued {
    /// Row id, also the message id of the email event (so a retry does not send it twice).
    pub id: Uuid,
    /// The secret, for the link. Never stored or logged.
    pub secret: String,
    /// When it stops working.
    pub expires_at: DateTime<Utc>,
}

/// Issues a token for `user_id`, retiring the unused ones of the same kind: the newest link
/// is the only one that works.
pub async fn issue(pool: &PgPool, kind: TokenKind, user_id: Uuid) -> Result<Issued, sqlx::Error> {
    let secret = opaque_token();
    let expires_at = Utc::now() + kind.ttl();
    let id = Uuid::now_v7();
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "UPDATE user_tokens SET used_at = now()
         WHERE user_id = $1 AND kind = $2 AND used_at IS NULL",
        user_id,
        kind.as_str(),
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "INSERT INTO user_tokens (id, kind, token_hash, user_id, expires_at)
         VALUES ($1, $2, $3, $4, $5)",
        id,
        kind.as_str(),
        sha256_hex(&secret),
        user_id,
        expires_at,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Issued {
        id,
        secret,
        expires_at,
    })
}

/// The user of a token that is still valid, without spending it. For checks that must happen
/// before the token is consumed (the new password's policy).
pub async fn peek(
    pool: &PgPool,
    kind: TokenKind,
    secret: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT user_id FROM user_tokens
         WHERE token_hash = $1 AND kind = $2 AND used_at IS NULL AND expires_at > now()",
        sha256_hex(secret.trim()),
        kind.as_str(),
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| row.user_id))
}

/// Spends a token and returns its user, or `None` when it is unknown, used or expired.
pub async fn consume(
    pool: &PgPool,
    kind: TokenKind,
    secret: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    let row = sqlx::query!(
        "UPDATE user_tokens SET used_at = now()
         WHERE token_hash = $1 AND kind = $2 AND used_at IS NULL AND expires_at > now()
         RETURNING user_id",
        sha256_hex(secret.trim()),
        kind.as_str(),
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| row.user_id))
}
