//! Redis cache of the status page, against the PostgreSQL and Redis of `just up`.

use std::io::Read;

use axum::Router;
use axum::body::{Body, Bytes};
use http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use taut_config::Settings;
use taut_status_bff::cache::{self, PageCache};
use tower::ServiceExt;
use uuid::Uuid;

/// What browsers and the load tests send.
const BROWSER: &str = "gzip, deflate, br";

/// URL of the Redis of `just up` (`REDIS_URL` from the environment or `.env`).
fn redis_url() -> String {
    Settings::load().expect("settings load").redis.url
}

/// A slug no other test (or earlier run still in the cache) uses.
fn unique_slug() -> String {
    format!("c-{}", Uuid::now_v7().simple())
}

/// A tenant with one monitor named `monitor`; returns the monitor id.
async fn page_with_monitor(pool: &PgPool, slug: &str, monitor: &str) -> Uuid {
    let tenant = Uuid::now_v7();
    sqlx::query("INSERT INTO tenants (id, slug, name) VALUES ($1, $2, 'Acme')")
        .bind(tenant)
        .bind(slug)
        .execute(pool)
        .await
        .unwrap();
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO monitors (id, tenant_id, name, kind, target)
         VALUES ($1, $2, $3, 'http', 'https://example.com')",
    )
    .bind(id)
    .bind(tenant)
    .bind(monitor)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn rename(pool: &PgPool, monitor: Uuid, name: &str) {
    sqlx::query("UPDATE monitors SET name = $2 WHERE id = $1")
        .bind(monitor)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
}

fn app(pool: PgPool, cache: PageCache) -> Router {
    let settings = Settings::default();
    let telemetry = taut_telemetry::init(taut_status_bff::SERVICE_NAME, &settings.telemetry)
        .expect("telemetry");
    taut_status_bff::app(&settings, pool, cache, telemetry.metrics().clone())
}

struct Page {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl Page {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|value| value.to_str().unwrap())
    }

    /// The JSON, decompressed when it came as brotli.
    fn json(&self) -> Value {
        let bytes = if self.header("content-encoding") == Some("br") {
            let mut json = Vec::new();
            brotli::Decompressor::new(&self.body[..], 4096)
                .read_to_end(&mut json)
                .unwrap();
            json
        } else {
            assert_eq!(self.header("content-encoding"), None);
            self.body.to_vec()
        };
        serde_json::from_slice(&bytes).unwrap()
    }

    fn monitor_name(&self) -> String {
        self.json()["monitors"][0]["name"]
            .as_str()
            .unwrap()
            .to_owned()
    }
}

async fn get(app: &Router, path: &str, accept_encoding: Option<&str>) -> Page {
    let mut request = Request::get(path);
    if let Some(accept) = accept_encoding {
        request = request.header("accept-encoding", accept);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    Page {
        status,
        headers,
        body,
    }
}

async fn redis() -> redis::aio::MultiplexedConnection {
    redis::Client::open(redis_url())
        .unwrap()
        .get_multiplexed_async_connection()
        .await
        .expect("the Redis of `just up`")
}

async fn ttl_of(slug: &str) -> i64 {
    redis::cmd("TTL")
        .arg(cache::key(slug))
        .query_async(&mut redis().await)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn a_cached_page_is_served_until_it_expires(pool: PgPool) {
    let slug = unique_slug();
    let web = page_with_monitor(&pool, &slug, "Web").await;
    let app = app(pool.clone(), PageCache::lazy(&redis_url(), 30).unwrap());

    let first = get(&app, &format!("/s/{slug}"), Some(BROWSER)).await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.header("content-encoding"), Some("br"));
    assert_eq!(first.header("vary"), Some("accept-encoding"));
    assert_eq!(first.header("cache-control"), Some("public, max-age=30"));
    assert_eq!(first.header("content-type"), Some("application/json"));
    assert_eq!(first.monitor_name(), "Web");
    let ttl = ttl_of(&slug).await;
    assert!((1..=30).contains(&ttl), "ttl {ttl}");

    // The database changes, the page does not until the key expires. The slug is
    // case-insensitive, and so is its key.
    rename(&pool, web, "Api").await;
    let second = get(&app, &format!("/s/{}", slug.to_uppercase()), Some(BROWSER)).await;
    assert_eq!(second.status, StatusCode::OK);
    assert_eq!(second.body, first.body);
    assert_eq!(second.monitor_name(), "Web");

    redis::cmd("DEL")
        .arg(cache::key(&slug))
        .query_async::<()>(&mut redis().await)
        .await
        .unwrap();
    assert_eq!(
        get(&app, &format!("/s/{slug}"), Some(BROWSER))
            .await
            .monitor_name(),
        "Api"
    );
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn clients_without_brotli_get_the_same_page_as_json(pool: PgPool) {
    let slug = unique_slug();
    page_with_monitor(&pool, &slug, "Web").await;
    let app = app(pool, PageCache::lazy(&redis_url(), 30).unwrap());
    let path = format!("/s/{slug}");

    let miss = get(&app, &path, None).await;
    assert_eq!(miss.status, StatusCode::OK);
    assert_eq!(miss.header("content-type"), Some("application/json"));
    let hit = get(&app, &path, None).await;
    let brotli = get(&app, &path, Some(BROWSER)).await;
    // Same generated_at: the second and third requests came from the cache.
    assert_eq!(hit.json(), miss.json());
    assert_eq!(brotli.json(), miss.json());
    assert_eq!(miss.json()["monitors"][0]["name"], "Web");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn without_the_cache_every_request_reads_the_database(pool: PgPool) {
    let slug = unique_slug();
    let web = page_with_monitor(&pool, &slug, "Web").await;
    let app = app(pool.clone(), PageCache::lazy(&redis_url(), 0).unwrap());
    let path = format!("/s/{slug}");

    assert_eq!(get(&app, &path, None).await.monitor_name(), "Web");
    rename(&pool, web, "Api").await;
    assert_eq!(get(&app, &path, None).await.monitor_name(), "Api");
    assert_eq!(ttl_of(&slug).await, -2, "nothing was written to Redis");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn unknown_slugs_are_not_cached(pool: PgPool) {
    let slug = unique_slug();
    let app = app(pool, PageCache::lazy(&redis_url(), 30).unwrap());

    let page = get(&app, &format!("/s/{slug}"), Some(BROWSER)).await;
    assert_eq!(page.status, StatusCode::NOT_FOUND);
    assert_eq!(ttl_of(&slug).await, -2);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn without_redis_the_page_comes_from_the_database(pool: PgPool) {
    let slug = unique_slug();
    let web = page_with_monitor(&pool, &slug, "Web").await;
    let app = app(
        pool.clone(),
        PageCache::lazy("redis://127.0.0.1:1", 30).unwrap(),
    );
    let path = format!("/s/{slug}");

    assert_eq!(get(&app, &path, None).await.monitor_name(), "Web");
    rename(&pool, web, "Api").await;
    let page = get(&app, &path, None).await;
    assert_eq!(page.status, StatusCode::OK);
    assert_eq!(page.monitor_name(), "Api");
}
