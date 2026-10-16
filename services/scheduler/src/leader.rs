//! Leader election with a Redis lock (`lock:scheduler`, docs/taut.md).
//!
//! Several scheduler replicas may run, but only the one holding the lock schedules; the
//! others wait and take over when the lock expires. The value is a random token so a
//! replica only renews or releases its own lock, never one another replica took over.

use std::time::Duration;

use redis::aio::ConnectionManager;
use redis::{RedisResult, Script};

/// Redis key of the scheduler lock.
pub const LOCK_KEY: &str = "lock:scheduler";

const RENEW: &str = r"
if redis.call('get', KEYS[1]) == ARGV[1] then
  return redis.call('pexpire', KEYS[1], ARGV[2])
end
return 0";

const RELEASE: &str = r"
if redis.call('get', KEYS[1]) == ARGV[1] then
  return redis.call('del', KEYS[1])
end
return 0";

/// A lock held for `ttl` unless renewed.
pub struct LeaderLock {
    connection: ConnectionManager,
    key: String,
    token: String,
    ttl_ms: u64,
}

impl LeaderLock {
    /// Lock on `key` with a fresh token.
    pub fn new(connection: ConnectionManager, key: &str, ttl: Duration) -> Self {
        Self {
            connection,
            key: key.to_owned(),
            token: ulid::Ulid::generate().to_string(),
            ttl_ms: u64::try_from(ttl.as_millis()).unwrap_or(10_000),
        }
    }

    /// Takes the lock if nobody holds it.
    pub async fn acquire(&mut self) -> RedisResult<bool> {
        let reply: Option<String> = redis::cmd("SET")
            .arg(&self.key)
            .arg(&self.token)
            .arg("NX")
            .arg("PX")
            .arg(self.ttl_ms)
            .query_async(&mut self.connection)
            .await?;
        Ok(reply.is_some())
    }

    /// Extends the lock; `false` means another replica holds it now.
    pub async fn renew(&mut self) -> RedisResult<bool> {
        let renewed: i64 = Script::new(RENEW)
            .key(&self.key)
            .arg(&self.token)
            .arg(self.ttl_ms)
            .invoke_async(&mut self.connection)
            .await?;
        Ok(renewed == 1)
    }

    /// Gives the lock up so another replica can lead without waiting for it to expire.
    pub async fn release(&mut self) -> RedisResult<()> {
        let _: i64 = Script::new(RELEASE)
            .key(&self.key)
            .arg(&self.token)
            .invoke_async(&mut self.connection)
            .await?;
        Ok(())
    }
}
