//! Sessions: an opaque refresh token in an `HttpOnly` cookie, its record in Redis.
//!
//! Keys, all with the refresh TTL:
//! - `session:{sha256(token)}` → the record of the session the token belongs to;
//! - `sid:{sid}` → hash of the token currently valid for that session;
//! - `rotated:{sha256(old token)}` → sid, so presenting a token that was already rotated is
//!   recognized as a replay and ends the whole session.

use axum_extra::extract::cookie::{Cookie, SameSite};
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use taut_identity::random::{opaque_token, sha256_hex};
use uuid::Uuid;

/// Name of the refresh cookie.
pub const COOKIE_NAME: &str = "taut_refresh";
/// The cookie only travels to the session routes.
pub const COOKIE_PATH: &str = "/v1/auth";

/// What Redis keeps per session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Session id, carried by every access token minted for it.
    pub sid: Uuid,
    /// Owner.
    pub user_id: Uuid,
    /// `users.token_version` when the session started.
    pub token_version: i32,
    /// Whether a second factor was passed.
    pub mfa: bool,
    /// Unix seconds of the last login or re-authentication.
    pub auth_time: i64,
}

/// Outcome of presenting a refresh token.
#[derive(Debug)]
pub enum Rotated {
    /// The token was valid; here is the session and the token that replaces it.
    Ok {
        /// The session.
        session: Session,
        /// New refresh token for the cookie.
        token: String,
    },
    /// The token was already rotated: a replay. The session is gone.
    Reused,
    /// Unknown or expired.
    Invalid,
}

/// Redis-backed session store.
#[derive(Clone)]
pub struct SessionStore {
    redis: ConnectionManager,
    ttl_secs: u64,
}

impl SessionStore {
    /// Store over `redis` whose sessions live `ttl_secs` without use.
    pub fn new(redis: ConnectionManager, ttl_secs: u64) -> Self {
        Self { redis, ttl_secs }
    }

    /// Seconds a session lives without use.
    pub fn ttl_secs(&self) -> u64 {
        self.ttl_secs
    }

    /// Starts a session and returns its refresh token.
    pub async fn create(&self, session: &Session) -> redis::RedisResult<String> {
        let token = opaque_token();
        self.store(session, &token).await?;
        Ok(token)
    }

    async fn store(&self, session: &Session, token: &str) -> redis::RedisResult<()> {
        let hash = sha256_hex(token);
        let record = serde_json::to_string(session).expect("a session serializes");
        let mut redis = self.redis.clone();
        redis::pipe()
            .atomic()
            .cmd("SET")
            .arg(format!("session:{hash}"))
            .arg(record)
            .arg("EX")
            .arg(self.ttl_secs)
            .ignore()
            .cmd("SET")
            .arg(format!("sid:{}", session.sid))
            .arg(&hash)
            .arg("EX")
            .arg(self.ttl_secs)
            .ignore()
            .exec_async(&mut redis)
            .await
    }

    /// Spends `token`: returns the session with a new token, or why it was rejected. A
    /// replayed token revokes the session it belonged to.
    pub async fn rotate(&self, token: &str) -> redis::RedisResult<Rotated> {
        let hash = sha256_hex(token.trim());
        let mut redis = self.redis.clone();
        let record: Option<String> = redis::cmd("GETDEL")
            .arg(format!("session:{hash}"))
            .query_async(&mut redis)
            .await?;
        let Some(record) = record else {
            let rotated: Option<String> = redis::cmd("GET")
                .arg(format!("rotated:{hash}"))
                .query_async(&mut redis)
                .await?;
            return Ok(match rotated.and_then(|sid| sid.parse::<Uuid>().ok()) {
                Some(sid) => {
                    self.revoke_sid(sid).await?;
                    Rotated::Reused
                }
                None => Rotated::Invalid,
            });
        };
        let Ok(session) = serde_json::from_str::<Session>(&record) else {
            return Ok(Rotated::Invalid);
        };
        let new_token = opaque_token();
        redis::cmd("SET")
            .arg(format!("rotated:{hash}"))
            .arg(session.sid.to_string())
            .arg("EX")
            .arg(self.ttl_secs)
            .exec_async(&mut redis)
            .await?;
        self.store(&session, &new_token).await?;
        Ok(Rotated::Ok {
            session,
            token: new_token,
        })
    }

    /// Ends the session `token` belongs to, if any.
    pub async fn revoke(&self, token: &str) -> redis::RedisResult<()> {
        let hash = sha256_hex(token.trim());
        let mut redis = self.redis.clone();
        let record: Option<String> = redis::cmd("GETDEL")
            .arg(format!("session:{hash}"))
            .query_async(&mut redis)
            .await?;
        if let Some(session) = record.and_then(|r| serde_json::from_str::<Session>(&r).ok()) {
            redis::cmd("DEL")
                .arg(format!("sid:{}", session.sid))
                .exec_async(&mut redis)
                .await?;
        }
        Ok(())
    }

    /// Ends session `sid` whatever token it has now.
    pub async fn revoke_sid(&self, sid: Uuid) -> redis::RedisResult<()> {
        let mut redis = self.redis.clone();
        let current: Option<String> = redis::cmd("GETDEL")
            .arg(format!("sid:{sid}"))
            .query_async(&mut redis)
            .await?;
        if let Some(hash) = current {
            redis::cmd("DEL")
                .arg(format!("session:{hash}"))
                .exec_async(&mut redis)
                .await?;
        }
        Ok(())
    }

    /// The refresh cookie carrying `token`.
    pub fn cookie(&self, token: String, secure: bool) -> Cookie<'static> {
        Cookie::build((COOKIE_NAME, token))
            .http_only(true)
            .secure(secure)
            .same_site(SameSite::Lax)
            .path(COOKIE_PATH)
            .max_age(time::Duration::seconds(
                i64::try_from(self.ttl_secs).unwrap_or(i64::MAX),
            ))
            .build()
    }

    /// A cookie that deletes the refresh cookie.
    pub fn clear_cookie(secure: bool) -> Cookie<'static> {
        Cookie::build((COOKIE_NAME, ""))
            .http_only(true)
            .secure(secure)
            .same_site(SameSite::Lax)
            .path(COOKIE_PATH)
            .removal()
            .build()
    }
}
