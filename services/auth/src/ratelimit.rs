//! Fixed-window counters in Redis for the abuse-prone routes.
//!
//! A key counts failures (or emails sent) and expires with its window; the first hit sets the
//! expiration, later hits only increment. Keys: `rl:login:acct:{email}`, `rl:login:ip:{ip}`,
//! `rl:email:{email}`, `rl:register:ip:{ip}`, `rl:reset:ip:{ip}`, `rl:mfa:{user_id}`.

use std::net::IpAddr;
use std::time::Duration;

use redis::aio::ConnectionManager;
use taut_http::AppError;

use crate::redis_error;

/// A limit: at most `max` hits per `window`.
#[derive(Debug, Clone, Copy)]
pub struct Limit {
    /// Hits allowed per window.
    pub max: u64,
    /// Length of the window.
    pub window: Duration,
}

/// Wrong passwords per account: five in fifteen minutes (docs/taut.md).
pub const LOGIN_PER_ACCOUNT: Limit = Limit {
    max: 5,
    window: Duration::from_mins(15),
};
/// Wrong passwords per address. Looser than the account limit so one office behind NAT does
/// not lock everyone out after five typos.
pub const LOGIN_PER_IP: Limit = Limit {
    max: 20,
    window: Duration::from_mins(15),
};
/// Wrong second-factor codes per user: five in fifteen minutes. A six-digit code has a
/// million values; without this, guessing one within its 90 s window would be feasible.
pub const MFA_PER_ACCOUNT: Limit = Limit {
    max: 5,
    window: Duration::from_mins(15),
};
/// Emails sent to one address per hour.
pub const EMAILS_PER_ADDRESS: Limit = Limit {
    max: 3,
    window: Duration::from_hours(1),
};
/// Registrations or reset requests per address per hour.
pub const REQUESTS_PER_IP: Limit = Limit {
    max: 10,
    window: Duration::from_hours(1),
};

/// Redis-backed limiter.
#[derive(Clone)]
pub struct Limiter {
    redis: ConnectionManager,
}

/// Key of the per-address limits; `unknown` when the address is not available.
pub fn ip_key(prefix: &str, ip: Option<IpAddr>) -> String {
    match ip {
        Some(ip) => format!("rl:{prefix}:ip:{ip}"),
        None => format!("rl:{prefix}:ip:unknown"),
    }
}

impl Limiter {
    /// Limiter over `redis`.
    pub fn new(redis: ConnectionManager) -> Self {
        Self { redis }
    }

    /// Fails with `429` when `key` already reached `limit`.
    pub async fn check(&self, key: &str, limit: Limit) -> Result<(), AppError> {
        let mut redis = self.redis.clone();
        let (count, ttl): (Option<u64>, i64) = redis::pipe()
            .cmd("GET")
            .arg(key)
            .cmd("TTL")
            .arg(key)
            .query_async(&mut redis)
            .await
            .map_err(|err| redis_error(&err))?;
        if count.unwrap_or(0) >= limit.max {
            metrics::counter!("auth_rate_limited_total", "key" => key_family(key)).increment(1);
            return Err(AppError::TooManyRequests {
                retry_after_secs: u64::try_from(ttl).unwrap_or(limit.window.as_secs()).max(1),
            });
        }
        Ok(())
    }

    /// Counts one hit on `key`; the window starts with the first hit.
    pub async fn hit(&self, key: &str, limit: Limit) -> Result<(), AppError> {
        let mut redis = self.redis.clone();
        redis::pipe()
            .atomic()
            .cmd("INCR")
            .arg(key)
            .ignore()
            .cmd("EXPIRE")
            .arg(key)
            .arg(limit.window.as_secs())
            .arg("NX")
            .ignore()
            .exec_async(&mut redis)
            .await
            .map_err(|err| redis_error(&err))
    }

    /// Checks, then counts: for limits on actions rather than on failures (emails sent).
    /// `Ok(false)` means the action must be skipped.
    pub async fn try_hit(&self, key: &str, limit: Limit) -> Result<bool, AppError> {
        match self.check(key, limit).await {
            Ok(()) => {
                self.hit(key, limit).await?;
                Ok(true)
            }
            Err(AppError::TooManyRequests { .. }) => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// Forgets `key`, after a successful login.
    pub async fn reset(&self, key: &str) -> Result<(), AppError> {
        let mut redis = self.redis.clone();
        redis::cmd("DEL")
            .arg(key)
            .exec_async(&mut redis)
            .await
            .map_err(|err| redis_error(&err))
    }
}

/// `rl:login:acct:ana@x.co` → `login:acct`, `rl:email:ana@x.co` → `email`: the key without
/// its identifier, a label with bounded cardinality that never carries an address or an id.
fn key_family(key: &str) -> String {
    let mut parts = key.split(':').skip(1);
    let prefix = parts.next().unwrap_or_default();
    match parts.next() {
        Some(kind @ ("acct" | "ip")) => format!("{prefix}:{kind}"),
        _ => prefix.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_families_drop_the_identifier() {
        assert_eq!(key_family("rl:login:acct:ana@x.co"), "login:acct");
        assert_eq!(key_family("rl:register:ip:10.0.0.1"), "register:ip");
        assert_eq!(key_family("rl:mfa:0192b4a8"), "mfa");
        assert_eq!(key_family("rl:email:ana@x.co"), "email");
        assert_eq!(key_family("rl:reset:ip:unknown"), "reset:ip");
    }

    #[test]
    fn ip_keys_handle_the_unknown_address() {
        assert_eq!(ip_key("login", None), "rl:login:ip:unknown");
        assert_eq!(
            ip_key("login", Some("10.1.2.3".parse().unwrap())),
            "rl:login:ip:10.1.2.3"
        );
    }
}
