//! Taut status BFF: the public status page of each tenant (F-46, F-47).
//!
//! `GET /s/{slug}` answers, without a session, the JSON the SPA paints at `/status/{slug}`:
//! the overall state, every monitor with its current status and 90 daily uptime bars, and
//! the open incidents plus the ones resolved in the last week. It never exposes targets,
//! IPs or failure causes (a cause such as `dns: could not resolve db.internal` would leak a
//! hostname). Responses may be cached for 30 seconds; the Redis cache arrives in week 13.

pub mod page;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::IntoResponse;
use axum::routing::get;
use chrono::{DateTime, Days, Utc};
use metrics_exporter_prometheus::PrometheusHandle;
use serde::Serialize;
use sqlx::PgPool;
use taut_config::Settings;
use taut_http::{AppError, Json, Path, Probe, ServiceInfo, health, layers, metrics};
use taut_model::MonitorStatus;
use uuid::Uuid;

use crate::page::{DayBar, Overall};

/// Name reported in logs, metrics and `/health`.
pub const SERVICE_NAME: &str = "taut-status-bff";

/// `Cache-Control` of every page.
const CACHE_CONTROL: &str = "public, max-age=30";
/// Resolved incidents stay on the page this long.
const RECENT_INCIDENT_DAYS: i32 = 7;

/// Router with `/health`, `/ready`, `/metrics` and `/s/{slug}`.
pub fn app(settings: &Settings, pool: PgPool, metrics_handle: PrometheusHandle) -> Router {
    let info = ServiceInfo::new(SERVICE_NAME, env!("CARGO_PKG_VERSION"), &settings.app);
    let pages = Router::new()
        .route("/s/{slug}", get(status_page))
        .with_state(pool.clone());
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
    State(pool): State<PgPool>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let page = build(&pool, &slug, Utc::now())
        .await?
        .ok_or_else(|| AppError::NotFound("status page not found".to_owned()))?;
    Ok((
        [(
            header::CACHE_CONTROL,
            HeaderValue::from_static(CACHE_CONTROL),
        )],
        Json(page),
    ))
}

/// Assembles the page of `slug` as of `now`; `None` when no tenant has that slug.
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
    .fetch_all(pool)
    .await?;

    let today = now.date_naive();
    let first = today
        .checked_sub_days(Days::new(u64::from(page::DAYS) - 1))
        .unwrap_or(today);
    let first_instant = first.and_time(chrono::NaiveTime::MIN).and_utc();
    let days = sqlx::query!(
        r#"
        SELECT monitor_id AS "monitor_id!", day AS "day!", checks AS "checks!",
               failures AS "failures!"
        FROM uptime_daily
        WHERE tenant_id = $1 AND day >= $2
        "#,
        tenant.id,
        first_instant
    )
    .fetch_all(pool)
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
    .fetch_all(pool)
    .await?;

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
