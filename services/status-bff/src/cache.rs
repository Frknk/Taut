//! Redis cache of the status pages (week 13).
//!
//! A page lives under `status:{slug}` for [`StatusSettings::cache_ttl_secs`] already
//! serialized and compressed with brotli, so a hit costs one Redis read and a copy: no
//! database, no JSON and no compression per request. Clients that do not accept brotli (every
//! browser does over HTTPS) get it decompressed, and the compression layer gzips it as usual.
//!
//! The cache trades freshness for capacity: a page may be up to the TTL old, so a new incident
//! shows up within that time (F-47 accepts the 30 s of the `Cache-Control` max-age). Unknown
//! slugs are not cached.
//!
//! Redis stays off the critical path, as in the api's rate limit: when it fails or answers
//! slower than [`REDIS_TIMEOUT`], the page is built from the database and the failure is
//! logged and counted (`status_page_cache_total{result="error"}`). A TTL of 0 turns the cache
//! off, which is how the load tests measure the page without it.
//!
//! [`StatusSettings::cache_ttl_secs`]: taut_config::StatusSettings::cache_ttl_secs

use std::io::{self, Read};
use std::time::Duration;

use axum::http::{HeaderMap, header};
use redis::aio::{ConnectionManager, ConnectionManagerConfig};

/// Longest a request waits for Redis before building the page from the database.
pub const REDIS_TIMEOUT: Duration = Duration::from_millis(250);
/// Brotli quality of a stored page. It is compressed once per TTL, not per request, so it can
/// go above the 4 of the compression layer: 9 costs about the same as 4 on a status page and
/// comes out 14 % smaller; 11 is barely smaller and several times slower.
const QUALITY: i32 = 9;
/// Brotli window, as a power of two (4 MiB, the format's default).
const WINDOW: i32 = 22;
/// Buffer size of the brotli decoder.
const DECODE_BUFFER: usize = 4096;

/// Why the cache could not answer.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// Redis answered with an error or the connection failed.
    #[error("redis: {0}")]
    Redis(#[from] redis::RedisError),
    /// Redis took longer than [`REDIS_TIMEOUT`].
    #[error("redis timed out")]
    Timeout,
}

/// Status pages in Redis, compressed with brotli.
#[derive(Clone)]
pub struct PageCache {
    /// `None` when the cache is off.
    redis: Option<ConnectionManager>,
    ttl_secs: u64,
}

impl PageCache {
    /// No cache: every request reads the database.
    pub fn disabled() -> Self {
        Self {
            redis: None,
            ttl_secs: 0,
        }
    }

    /// Cache that connects on first use with short timeouts, so status-bff starts and keeps
    /// answering while Redis is down. A `ttl_secs` of 0 is [`PageCache::disabled`].
    pub fn lazy(url: &str, ttl_secs: u64) -> redis::RedisResult<Self> {
        if ttl_secs == 0 {
            return Ok(Self::disabled());
        }
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(REDIS_TIMEOUT))
            .set_response_timeout(Some(REDIS_TIMEOUT))
            .set_number_of_retries(1);
        let redis = redis::Client::open(url)?.get_connection_manager_lazy(config)?;
        Ok(Self {
            redis: Some(redis),
            ttl_secs,
        })
    }

    /// Whether pages are cached.
    pub fn is_enabled(&self) -> bool {
        self.redis.is_some()
    }

    /// Seconds a page lives in the cache; 0 when it is off.
    pub fn ttl_secs(&self) -> u64 {
        self.ttl_secs
    }

    /// The stored page of `slug`, compressed; `None` on a miss or with the cache off.
    pub async fn get(&self, slug: &str) -> Result<Option<Vec<u8>>, CacheError> {
        let Some(redis) = &self.redis else {
            return Ok(None);
        };
        let mut redis = redis.clone();
        let mut cmd = redis::cmd("GET");
        cmd.arg(key(slug));
        let read = cmd.query_async::<Option<Vec<u8>>>(&mut redis);
        Ok(tokio::time::timeout(REDIS_TIMEOUT, read)
            .await
            .map_err(|_| CacheError::Timeout)??)
    }

    /// Stores the compressed `page` of `slug` for the TTL; does nothing with the cache off.
    pub async fn put(&self, slug: &str, page: &[u8]) -> Result<(), CacheError> {
        let Some(redis) = &self.redis else {
            return Ok(());
        };
        let mut redis = redis.clone();
        let mut cmd = redis::cmd("SET");
        cmd.arg(key(slug)).arg(page).arg("EX").arg(self.ttl_secs);
        let write = cmd.query_async::<()>(&mut redis);
        Ok(tokio::time::timeout(REDIS_TIMEOUT, write)
            .await
            .map_err(|_| CacheError::Timeout)??)
    }
}

/// Redis key of the page of `slug` (already lowercase).
pub fn key(slug: &str) -> String {
    format!("status:{slug}")
}

/// `json` compressed with brotli at [`QUALITY`].
pub fn compress(json: &[u8]) -> io::Result<Vec<u8>> {
    let params = brotli::enc::BrotliEncoderParams {
        quality: QUALITY,
        lgwin: WINDOW,
        ..Default::default()
    };
    let mut out = Vec::with_capacity(json.len() / 8);
    brotli::BrotliCompress(&mut &json[..], &mut out, &params)?;
    Ok(out)
}

/// The JSON of a page stored by [`compress`].
pub fn decompress(page: &[u8]) -> io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(page.len() * 16);
    brotli::Decompressor::new(page, DECODE_BUFFER).read_to_end(&mut out)?;
    Ok(out)
}

/// Whether `Accept-Encoding` allows brotli: a `br` entry whose `q` is not 0. A bare `*` does
/// not count; such clients get the page the compression layer would give them.
pub fn accepts_brotli(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|entry| {
            let (coding, params) = entry.split_once(';').unwrap_or((entry, ""));
            coding.trim().eq_ignore_ascii_case("br") && !refused(params)
        })
}

/// Whether the parameters of an `Accept-Encoding` entry carry `q=0`.
fn refused(params: &str) -> bool {
    params.split(';').any(|param| {
        let param = param.trim();
        param
            .strip_prefix("q=")
            .or_else(|| param.strip_prefix("Q="))
            .is_some_and(|q| q.trim().parse::<f32>().is_ok_and(|q| q <= 0.0))
    })
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn accepts(values: &[&'static str]) -> bool {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(header::ACCEPT_ENCODING, HeaderValue::from_static(value));
        }
        accepts_brotli(&headers)
    }

    #[test]
    fn brotli_is_accepted_when_listed() {
        assert!(accepts(&["gzip, deflate, br"]));
        assert!(accepts(&["br"]));
        assert!(accepts(&["BR;q=0.5"]));
        assert!(accepts(&["gzip", "br;q=1"]));
        assert!(accepts(&["gzip;q=1.0, br; q=0.1"]));
    }

    #[test]
    fn brotli_is_refused_when_missing_or_at_q_zero() {
        assert!(!accepts(&[]));
        assert!(!accepts(&["gzip, deflate"]));
        assert!(!accepts(&["identity"]));
        assert!(!accepts(&["*"]));
        assert!(!accepts(&["br;q=0"]));
        assert!(!accepts(&["gzip, br;q=0.000"]));
        assert!(!accepts(&["brotli"]));
    }

    #[test]
    fn compression_round_trips() {
        let json = br#"{"tenant":{"name":"Acme","slug":"acme"},"monitors":[]}"#.repeat(200);
        let page = compress(&json).unwrap();
        assert!(page.len() < json.len() / 10);
        assert_eq!(decompress(&page).unwrap(), json);
    }

    #[tokio::test]
    async fn a_ttl_of_zero_turns_the_cache_off() {
        let cache = PageCache::lazy("redis://127.0.0.1:1", 0).unwrap();
        assert!(!cache.is_enabled());
        assert!(
            PageCache::lazy("redis://127.0.0.1:1", 30)
                .unwrap()
                .is_enabled()
        );
    }

    #[test]
    fn keys_follow_the_design() {
        assert_eq!(key("acme"), "status:acme");
    }
}
