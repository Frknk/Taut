//! The `users` table.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use taut_http::AppError;
use uuid::Uuid;

/// Longest display name.
pub const MAX_NAME_CHARS: usize = 100;
/// Longest email address (RFC 5321).
pub const MAX_EMAIL_CHARS: usize = 254;

/// A user row.
#[derive(Debug, Clone)]
pub struct User {
    /// Identifier, a UUID v7.
    pub id: Uuid,
    /// Lowercase address.
    pub email: String,
    /// Display name.
    pub name: String,
    /// Argon2id PHC string.
    pub password_hash: String,
    /// When the address was confirmed; `None` until then.
    pub email_verified_at: Option<DateTime<Utc>>,
    /// Bumped on every password change; sessions carry the version they started with.
    pub token_version: i32,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// What the API tells a user about themselves.
#[derive(Debug, Clone, Serialize)]
pub struct PublicUser {
    /// Identifier.
    pub id: Uuid,
    /// Address.
    pub email: String,
    /// Display name.
    pub name: String,
    /// When the address was confirmed.
    pub email_verified_at: Option<DateTime<Utc>>,
    /// Whether a second factor is enrolled (always `false` until 2FA ships).
    pub mfa_enabled: bool,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

impl User {
    /// Public view.
    pub fn public(&self) -> PublicUser {
        PublicUser {
            id: self.id,
            email: self.email.clone(),
            name: self.name.clone(),
            email_verified_at: self.email_verified_at,
            mfa_enabled: false,
            created_at: self.created_at,
        }
    }
}

/// Trimmed, lowercased address, or a `400` when it does not look like one. The check is
/// deliberately loose: the verification email is the real test.
pub fn normalize_email(email: &str) -> Result<String, AppError> {
    let email = email.trim().to_lowercase();
    let valid = email.chars().count() <= MAX_EMAIL_CHARS
        && !email.chars().any(char::is_whitespace)
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && !domain.is_empty()
                && !domain.contains('@')
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        });
    if valid {
        Ok(email)
    } else {
        Err(AppError::BadRequest(
            "email is not a valid address".to_owned(),
        ))
    }
}

/// Trimmed display name, or a `400`.
pub fn normalize_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(AppError::BadRequest(format!(
            "name must be between 1 and {MAX_NAME_CHARS} characters"
        )));
    }
    Ok(name.to_owned())
}

/// Looks a user up by normalized email.
pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        "SELECT id, email, name, password_hash, email_verified_at, token_version, created_at
         FROM users WHERE email = $1",
        email
    )
    .fetch_optional(pool)
    .await
}

/// Looks a user up by id.
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        "SELECT id, email, name, password_hash, email_verified_at, token_version, created_at
         FROM users WHERE id = $1",
        id
    )
    .fetch_optional(pool)
    .await
}

/// Inserts a user; `None` when the email is already taken.
pub async fn insert(
    pool: &PgPool,
    email: &str,
    name: &str,
    password_hash: &str,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        "INSERT INTO users (id, email, name, password_hash) VALUES ($1, $2, $3, $4)
         ON CONFLICT (email) DO NOTHING
         RETURNING id, email, name, password_hash, email_verified_at, token_version, created_at",
        Uuid::now_v7(),
        email,
        name,
        password_hash,
    )
    .fetch_optional(pool)
    .await
}

/// Marks the address as verified (idempotent).
pub async fn mark_verified(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE users SET email_verified_at = coalesce(email_verified_at, now()), updated_at = now()
         WHERE id = $1",
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Stores a new password hash and bumps `token_version`, which ends every existing session.
/// `verify` also confirms the address: whoever resets a password proved they read the email.
/// Returns the new version.
pub async fn set_password(
    pool: &PgPool,
    id: Uuid,
    password_hash: &str,
    verify: bool,
) -> Result<i32, sqlx::Error> {
    let row = sqlx::query!(
        "UPDATE users
         SET password_hash = $2,
             token_version = token_version + 1,
             email_verified_at = CASE WHEN $3 THEN coalesce(email_verified_at, now())
                                      ELSE email_verified_at END,
             updated_at = now()
         WHERE id = $1
         RETURNING token_version",
        id,
        password_hash,
        verify,
    )
    .fetch_one(pool)
    .await?;
    Ok(row.token_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails_are_normalized_and_checked_loosely() {
        assert_eq!(
            normalize_email("  Ana@Example.COM ").unwrap(),
            "ana@example.com"
        );
        for bad in [
            "",
            "ana",
            "ana@",
            "@example.com",
            "ana@example",
            "a na@x.co",
            "a@b@c.co",
        ] {
            assert!(normalize_email(bad).is_err(), "{bad:?} should be invalid");
        }
    }

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(normalize_name("  Ana ").unwrap(), "Ana");
        assert!(normalize_name("   ").is_err());
        assert!(normalize_name(&"x".repeat(101)).is_err());
    }
}
