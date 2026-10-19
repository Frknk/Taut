//! Per-tenant request budgets: fixed one-minute windows in Redis.
//!
//! Every request that resolves a tenant ([`crate::tenant::CurrentTenant`]) counts against one
//! of two budgets of that tenant. API keys get the plan's documented quota
//! ([`Plan::api_key_requests_per_minute`], 60 on the free plan); the panel gets its own,
//! looser one ([`Plan::panel_requests_per_minute`]), because every open tab polls and members
//! share it. Over budget the answer is `429 too_many_requests` with `Retry-After` set to the
//! end of the minute. Keys: `rl:{tenant}:{minute}` and `rl:{tenant}:panel:{minute}`, expiring
//! with their window.
//!
//! The limit protects capacity, not data, so Redis stays off the critical path: when it fails
//! or answers slower than [`REDIS_TIMEOUT`], the request goes through and the failure is
//! logged and counted (`api_rate_limit_errors_total`).
//!
//! [`Plan::api_key_requests_per_minute`]: crate::tenant::Plan::api_key_requests_per_minute
//! [`Plan::panel_requests_per_minute`]: crate::tenant::Plan::panel_requests_per_minute

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use taut_http::AppError;
use uuid::Uuid;

/// Length of a window.
const WINDOW_SECS: u64 = 60;
/// Longest a request waits for Redis before going through uncounted.
pub const REDIS_TIMEOUT: Duration = Duration::from_millis(250);

/// Which budget of the tenant a request spends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    /// Requests with a tenant API key (`Bearer taut_live_…`).
    ApiKey,
    /// Requests with a user's access token: the panel.
    Panel,
}

impl Bucket {
    /// Metric label.
    const fn label(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::Panel => "panel",
        }
    }
}

/// Counts requests per tenant in Redis.
#[derive(Clone)]
pub struct TenantLimiter {
    redis: ConnectionManager,
}

impl TenantLimiter {
    /// Limiter over an existing connection.
    pub fn new(redis: ConnectionManager) -> Self {
        Self { redis }
    }

    /// Limiter that connects on first use, with short timeouts: the api starts and keeps
    /// answering while Redis is down.
    pub fn lazy(url: &str) -> redis::RedisResult<Self> {
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(REDIS_TIMEOUT))
            .set_response_timeout(Some(REDIS_TIMEOUT))
            .set_number_of_retries(1);
        let redis = redis::Client::open(url)?.get_connection_manager_lazy(config)?;
        Ok(Self { redis })
    }

    /// Counts one request of `tenant` in `bucket`; `429` once the window holds more than
    /// `budget`. Never fails because of Redis.
    pub async fn hit(&self, tenant: Uuid, bucket: Bucket, budget: u64) -> Result<(), AppError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let (key, retry_after_secs) = window(tenant, bucket, now);
        let mut redis = self.redis.clone();
        let counted = tokio::time::timeout(
            REDIS_TIMEOUT,
            redis::pipe()
                .atomic()
                .cmd("INCR")
                .arg(&key)
                .cmd("EXPIRE")
                .arg(&key)
                .arg(WINDOW_SECS)
                .arg("NX")
                .ignore()
                .query_async::<(u64,)>(&mut redis),
        )
        .await;
        match counted {
            Ok(Ok((count,))) if count > budget => {
                metrics::counter!("api_rate_limited_total", "bucket" => bucket.label())
                    .increment(1);
                Err(AppError::TooManyRequests { retry_after_secs })
            }
            Ok(Ok(_)) => Ok(()),
            Ok(Err(err)) => {
                unavailable(&err.to_string());
                Ok(())
            }
            Err(_) => {
                unavailable("timed out");
                Ok(())
            }
        }
    }
}

fn unavailable(error: &str) {
    tracing::warn!(
        error,
        "rate limit unavailable; the request goes through uncounted"
    );
    metrics::counter!("api_rate_limit_errors_total").increment(1);
}

/// Key of the window that holds `now` (unix seconds) and the seconds until it ends.
pub fn window(tenant: Uuid, bucket: Bucket, now: u64) -> (String, u64) {
    let minute = now / WINDOW_SECS;
    let key = match bucket {
        Bucket::ApiKey => format!("rl:{tenant}:{minute}"),
        Bucket::Panel => format!("rl:{tenant}:panel:{minute}"),
    };
    (key, WINDOW_SECS - now % WINDOW_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TENANT: Uuid = Uuid::from_u128(0x0192_b4a8_0000_7000_8000_0000_0000_0001);

    #[test]
    fn windows_are_whole_minutes() {
        assert_eq!(
            window(TENANT, Bucket::ApiKey, 1_700_000_040),
            (
                "rl:0192b4a8-0000-7000-8000-000000000001:28333334".to_owned(),
                60
            )
        );
        assert_eq!(window(TENANT, Bucket::ApiKey, 1_700_000_099).1, 1);
        assert_eq!(
            window(TENANT, Bucket::ApiKey, 1_700_000_100).0,
            "rl:0192b4a8-0000-7000-8000-000000000001:28333335"
        );
    }

    #[test]
    fn the_panel_has_its_own_key() {
        assert_eq!(
            window(TENANT, Bucket::Panel, 1_700_000_040).0,
            "rl:0192b4a8-0000-7000-8000-000000000001:panel:28333334"
        );
    }
}
