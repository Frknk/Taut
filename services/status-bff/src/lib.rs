//! Taut status BFF: the public status page of each tenant (F-46, F-47).
//!
//! `GET /s/{slug}` answers, without a session, the JSON the SPA paints at `/status/{slug}`:
//! the overall state, every monitor with its current status and 90 daily uptime bars, and
//! the open incidents plus the ones resolved in the last week. It never exposes targets,
//! IPs or failure causes (a cause such as `dns: could not resolve db.internal` would leak a
//! hostname). Pages are cached in Redis for 30 seconds, already compressed, with the last copy
//! of each in memory to serve while it is rebuilt ([`cache`]); browsers may keep them for as
//! long (`Cache-Control: max-age=30`).

pub mod cache;
pub mod page;

use anyhow::Context;
use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use chrono::{DateTime, Days, Utc};
use metrics_exporter_prometheus::PrometheusHandle;
use serde::Serialize;
use sqlx::PgPool;
use taut_config::Settings;
use taut_http::{AppError, Json, Path, Probe, ServiceInfo, health, layers, metrics};
use taut_model::MonitorStatus;
use uuid::Uuid;

use crate::cache::PageCache;
use crate::page::{DayBar, Overall};

/// Name reported in logs, metrics and `/health`.
pub const SERVICE_NAME: &str = "taut-status-bff";

/// `Cache-Control` of every page.
const CACHE_CONTROL: &str = "public, max-age=30";
/// Resolved incidents stay on the page this long.
const RECENT_INCIDENT_DAYS: i32 = 7;

/// What the page handler needs.
#[derive(Clone)]
struct Pages {
    pool: PgPool,
    cache: PageCache,
}

/// Router with `/health`, `/ready`, `/metrics` and `/s/{slug}`. Redis has no readiness probe:
/// without it the pages are still served, from the database.
pub fn app(
    settings: &Settings,
    pool: PgPool,
    cache: PageCache,
    metrics_handle: PrometheusHandle,
) -> Router {
    let info = ServiceInfo::new(SERVICE_NAME, env!("CARGO_PKG_VERSION"), &settings.app);
    let pages = Router::new()
        .route("/s/{slug}", get(status_page))
        .with_state(Pages {
            pool: pool.clone(),
            cache,
        });
    let router = Router::new()
        .merge(health::router(info, vec![Probe::database(pool)]))
        .merge(metrics::router(metrics_handle))
        .merge(pages)
        .fallback(not_found);
    layers::apply(router, &settings.http)
}

#[allow(clippy::unused_async)] // axum handlers must be async
async fn not_found() -> AppError {
    AppError::NotFound("route not found".to_owned())
}

/// Public JSON of a status page.
#[derive(Debug, Serialize)]
pub struct StatusPage {
    /// Tenant name and slug.
    pub tenant: PublicTenant,
    /// Overall state.
    pub status: Overall,
    /// Monitors in creation order.
    pub monitors: Vec<PublicMonitor>,
    /// Open incidents first, then the ones resolved in the last week.
    pub incidents: Vec<PublicIncident>,
    /// When the page was assembled.
    pub generated_at: DateTime<Utc>,
}

/// What the page says about the tenant.
#[derive(Debug, Serialize)]
pub struct PublicTenant {
    /// Display name.
    pub name: String,
    /// Slug of the page.
    pub slug: String,
}

/// What the page says about a monitor: no target, no cause.
#[derive(Debug, Serialize)]
pub struct PublicMonitor {
    /// Display name.
    pub name: String,
    /// Current status.
    pub status: MonitorStatus,
    /// Uptime over the 90 days, `None` without checks.
    pub uptime_90d: Option<f64>,
    /// One bar per day, oldest first.
    pub days: Vec<DayBar>,
}

/// What the page says about an incident.
#[derive(Debug, Serialize)]
pub struct PublicIncident {
    /// Affected monitor.
    pub monitor_name: String,
    /// Start.
    pub started_at: DateTime<Utc>,
    /// End, `None` while open.
    pub resolved_at: Option<DateTime<Utc>>,
    /// Seconds it lasted or has lasted so far.
    pub duration_s: i64,
}

struct MonitorRow {
    id: Uuid,
    name: String,
    status: MonitorStatus,
}

async fn status_page(
    State(Pages { pool, cache }): State<Pages>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let slug = slug.to_ascii_lowercase();
    let brotli = cache::accepts_brotli(&headers);
    if !cache.is_enabled() {
        served("database");
        return Ok(uncached(fresh(&pool, &slug).await?));
    }

    match cache.get(&slug).await {
        Ok(Some(page)) => {
            count("hit");
            served("redis");
            let page = Bytes::from(page);
            if cache
                .recall(&slug)
                .is_none_or(|kept| kept.age >= cache.ttl())
            {
                cache.remember(&slug, page.clone());
            }
            return stored(&page, brotli);
        }
        Ok(None) => count("miss"),
        Err(err) => cache_failed(&err),
    }

    // Redis has no page for us: the copy in memory answers while it is not too old.
    if let Some(kept) = cache.recall(&slug) {
        if kept.age < cache.ttl() {
            served("memory");
            return stored(&kept.page, brotli);
        }
        if kept.age < cache::MAX_STALE {
            served("stale");
            rebuild_in_background(&pool, &cache, &slug);
            return stored(&kept.page, brotli);
        }
    }

    // No usable copy: one request rebuilds the page and the others wait for it.
    let flight = cache.flight(&slug);
    let _rebuilding = flight.lock().await;
    if let Some(kept) = cache.recall(&slug).filter(|kept| kept.age < cache.ttl()) {
        served("memory");
        return stored(&kept.page, brotli);
    }
    served("database");
    let page = rebuild(&pool, &cache, &slug).await?;
    stored(&page, brotli)
}

/// Builds the page of `slug`, compresses it off the async workers and stores it in Redis and in
/// memory. The caller holds the flight of `slug`.
async fn rebuild(pool: &PgPool, cache: &PageCache, slug: &str) -> Result<Bytes, AppError> {
    let page = fresh(pool, slug).await?;
    // Serialized and compressed once per TTL, then served as is.
    let compressed = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        let json = serde_json::to_vec(&page).context("serialize status page")?;
        cache::compress(&json).context("compress status page")
    })
    .await
    .context("compress status page")??;
    if let Err(err) = cache.put(slug, &compressed).await {
        cache_failed(&err);
    }
    let compressed = Bytes::from(compressed);
    cache.remember(slug, compressed.clone());
    Ok(compressed)
}

/// Rebuilds the page of `slug` in a task of its own, unless one is already doing it.
fn rebuild_in_background(pool: &PgPool, cache: &PageCache, slug: &str) {
    let Ok(rebuilding) = cache.flight(slug).try_lock_owned() else {
        return;
    };
    let (pool, cache, slug) = (pool.clone(), cache.clone(), slug.to_owned());
    tokio::spawn(async move {
        if let Err(err) = rebuild(&pool, &cache, &slug).await {
            tracing::warn!(error = %err, "status page rebuild failed; serving the old copy");
        }
        drop(rebuilding);
    });
}

/// The page of `slug` built from the database; `404` for an unknown slug.
async fn fresh(pool: &PgPool, slug: &str) -> Result<StatusPage, AppError> {
    build(pool, slug, Utc::now())
        .await?
        .ok_or_else(|| AppError::NotFound("status page not found".to_owned()))
}

/// A page as served without the cache: the compression layer compresses it per request.
fn uncached(page: StatusPage) -> Response {
    (
        [(
            header::CACHE_CONTROL,
            HeaderValue::from_static(CACHE_CONTROL),
        )],
        Json(page),
    )
        .into_response()
}

/// A stored page: as is for brotli clients, decompressed for the rest.
fn stored(page: &Bytes, brotli: bool) -> Result<Response, AppError> {
    if brotli {
        return Ok(encoded(page.clone()));
    }
    let json = cache::decompress(page).context("decompress cached status page")?;
    Ok(plain(json))
}

/// Brotli bytes as they are. The compression layer leaves a response that already has a
/// `Content-Encoding` alone, so `Vary` goes here.
fn encoded(page: Bytes) -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ),
            (header::CONTENT_ENCODING, HeaderValue::from_static("br")),
            (header::VARY, HeaderValue::from_static("accept-encoding")),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static(CACHE_CONTROL),
            ),
        ],
        page,
    )
        .into_response()
}

/// JSON bytes; the compression layer still gzips them if the client asks.
fn plain(json: Vec<u8>) -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static(CACHE_CONTROL),
            ),
        ],
        json,
    )
        .into_response()
}

/// Counts a Redis lookup: `hit`, `miss` or `error`.
fn count(result: &'static str) {
    ::metrics::counter!("status_page_cache_total", "result" => result).increment(1);
}

/// Counts where a page came from: `redis`, `memory`, `stale` (an old copy served while it is
/// rebuilt) or `database`.
fn served(source: &'static str) {
    ::metrics::counter!("status_page_served_total", "source" => source).increment(1);
}

fn cache_failed(err: &cache::CacheError) {
    tracing::warn!(error = %err, "status page cache unavailable");
    count("error");
}

/// Assembles the page of `slug` as of `now`; `None` when no tenant has that slug. The slug is
/// looked up across tenants (it is what names one); everything else is read in a transaction
/// of that tenant, so row-level security keeps the page to its own monitors and incidents.
pub async fn build(
    pool: &PgPool,
    slug: &str,
    now: DateTime<Utc>,
) -> Result<Option<StatusPage>, sqlx::Error> {
    let Some(tenant) = sqlx::query!(
        "SELECT id, name, slug FROM tenants WHERE slug = $1",
        slug.to_ascii_lowercase()
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    let mut tx = taut_db::begin_tenant(pool, tenant.id).await?;
    let monitors = sqlx::query_as!(
        MonitorRow,
        r#"
        SELECT m.id, m.name,
               CASE WHEN NOT m.active THEN 'paused' ELSE coalesce(s.status, 'pending') END
                   AS "status!: MonitorStatus"
        FROM monitors m
        LEFT JOIN monitor_status s ON s.monitor_id = m.id
        WHERE m.tenant_id = $1
        ORDER BY m.created_at
        "#,
        tenant.id
    )
    .fetch_all(&mut *tx)
    .await?;

    let today = now.date_naive();
    let first = today
        .checked_sub_days(Days::new(u64::from(page::DAYS) - 1))
        .unwrap_or(today);
    let first_instant = first.and_time(chrono::NaiveTime::MIN).and_utc();
    let ids: Vec<Uuid> = monitors.iter().map(|monitor| monitor.id).collect();
    // The continuous aggregate has no row-level security of its own; this function reads it
    // for the transaction's tenant only. The ids let today's part, not materialized yet, use
    // the primary key of check_results instead of reading the whole day of every tenant.
    let days = sqlx::query!(
        r#"
        SELECT monitor_id AS "monitor_id!", day AS "day!", checks AS "checks!",
               failures AS "failures!"
        FROM uptime_daily_of_monitors($1, $2)
        "#,
        first_instant,
        &ids,
    )
    .fetch_all(&mut *tx)
    .await?;

    let incidents = sqlx::query_as!(
        PublicIncident,
        r#"
        SELECT m.name AS monitor_name, i.started_at, i.resolved_at,
               floor(extract(epoch FROM coalesce(i.resolved_at, $3) - i.started_at))::bigint
                   AS "duration_s!"
        FROM incidents i
        JOIN monitors m ON m.id = i.monitor_id
        WHERE i.tenant_id = $1
          AND (i.resolved_at IS NULL OR i.resolved_at > $3 - make_interval(days => $2))
        ORDER BY i.resolved_at IS NULL DESC, i.started_at DESC
        LIMIT 20
        "#,
        tenant.id,
        RECENT_INCIDENT_DAYS,
        now
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    let statuses: Vec<MonitorStatus> = monitors.iter().map(|monitor| monitor.status).collect();
    let monitors = monitors
        .into_iter()
        .map(|monitor| {
            let rows: Vec<_> = days
                .iter()
                .filter(|row| row.monitor_id == monitor.id)
                .map(|row| (row.day.date_naive(), row.checks, row.failures))
                .collect();
            let bars = page::bars(first, &rows);
            PublicMonitor {
                name: monitor.name,
                status: monitor.status,
                uptime_90d: page::overall_uptime(&bars),
                days: bars,
            }
        })
        .collect();

    Ok(Some(StatusPage {
        tenant: PublicTenant {
            name: tenant.name,
            slug: tenant.slug,
        },
        status: page::overall(&statuses),
        monitors,
        incidents,
        generated_at: now,
    }))
}
