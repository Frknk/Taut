//! `GET /v1/monitors/{id}/metrics`: latency and uptime of a monitor (F-26).
//!
//! The series and the percentiles come from the `check_results` hypertable with
//! `time_bucket` and `percentile_cont`. The uptime of 7 and 30 days, which every response
//! carries, adds the whole days of `uptime_daily` to the raw results of the partial first
//! day: reading 30 days of raw results on every call cost ten times the other reads
//! (`docs/loadtest/2026-10-19-api-crud.md`). The CQRS split moves these reads to their own
//! service in week 10. Latency only counts healthy checks, so timeouts do not drown the
//! percentiles.

// Axum handlers receive their state by value even when they only borrow it.
#![allow(clippy::needless_pass_by_value)]

use axum::Router;
use axum::extract::State;
use axum::routing::get;
use chrono::{DateTime, Days, NaiveTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use taut_http::{AppError, Json, Path, Query};
use taut_model::uptime_percent;
use uuid::Uuid;

use crate::AppState;
use crate::monitors;
use crate::tenant::CurrentTenant;

/// Route `/v1/monitors/{id}/metrics`.
pub fn router() -> Router<AppState> {
    Router::new().route("/v1/monitors/{id}/metrics", get(metrics))
}

/// Window of the series.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub enum Range {
    /// Last 24 hours in 15-minute buckets.
    #[default]
    #[serde(rename = "24h")]
    Day,
    /// Last 7 days in 2-hour buckets.
    #[serde(rename = "7d")]
    Week,
    /// Last 30 days in 12-hour buckets.
    #[serde(rename = "30d")]
    Month,
}

impl Range {
    const fn name(self) -> &'static str {
        match self {
            Self::Day => "24h",
            Self::Week => "7d",
            Self::Month => "30d",
        }
    }

    const fn window_s(self) -> i64 {
        match self {
            Self::Day => 24 * 3600,
            Self::Week => 7 * 24 * 3600,
            Self::Month => 30 * 24 * 3600,
        }
    }

    const fn bucket_s(self) -> i64 {
        match self {
            Self::Day => 15 * 60,
            Self::Week => 2 * 3600,
            Self::Month => 12 * 3600,
        }
    }
}

#[derive(Debug, Deserialize)]
struct MetricsQuery {
    #[serde(default)]
    range: Range,
}

/// Uptime of the last day, week and month, in percent; `None` without checks.
#[derive(Debug, Serialize)]
pub struct Uptimes {
    /// Last 24 hours.
    #[serde(rename = "24h")]
    pub day: Option<f64>,
    /// Last 7 days.
    #[serde(rename = "7d")]
    pub week: Option<f64>,
    /// Last 30 days.
    #[serde(rename = "30d")]
    pub month: Option<f64>,
}

/// Latency of the healthy checks of the range, in milliseconds.
#[derive(Debug, Serialize)]
pub struct Latency {
    /// Median.
    pub p50_ms: Option<f64>,
    /// 95th percentile.
    pub p95_ms: Option<f64>,
    /// Mean.
    pub avg_ms: Option<f64>,
}

/// One bucket of the series.
#[derive(Debug, Serialize)]
pub struct Point {
    /// Start of the bucket.
    pub t: DateTime<Utc>,
    /// Checks in the bucket.
    pub checks: i64,
    /// Failed checks in the bucket.
    pub failures: i64,
    /// Mean latency of the healthy checks.
    pub avg_ms: Option<f64>,
    /// 95th percentile of the healthy checks.
    pub p95_ms: Option<f64>,
}

/// Body of the response.
#[derive(Debug, Serialize)]
pub struct Metrics {
    /// `24h`, `7d` or `30d`.
    pub range: &'static str,
    /// Seconds per bucket of `series`.
    pub bucket_s: i64,
    /// Checks in the range.
    pub checks: i64,
    /// Failed checks in the range.
    pub failures: i64,
    /// Uptime of the three standard windows, whatever the range.
    pub uptime: Uptimes,
    /// Latency over the range.
    pub latency: Latency,
    /// Buckets with at least one check, oldest first.
    pub series: Vec<Point>,
}

async fn metrics(
    State(state): State<AppState>,
    CurrentTenant { tenant, .. }: CurrentTenant,
    Path(id): Path<Uuid>,
    Query(query): Query<MetricsQuery>,
) -> Result<Json<Metrics>, AppError> {
    let mut tx = state.tenant_tx(tenant.id).await?;
    if monitors::find(&mut tx, tenant.id, id).await?.is_none() {
        return Err(AppError::NotFound("monitor not found".to_owned()));
    }
    let range = query.range;
    #[allow(clippy::cast_precision_loss)] // a few million seconds at most
    let (window_s, bucket_s) = (range.window_s() as f64, range.bucket_s() as f64);

    let counts = uptime_counts(&mut tx, tenant.id, id, Utc::now()).await?;

    let latency = sqlx::query_as!(
        Latency,
        r#"
        SELECT
            percentile_cont(0.5) WITHIN GROUP (ORDER BY latency_ms) FILTER (WHERE ok) AS p50_ms,
            percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms) FILTER (WHERE ok) AS p95_ms,
            (avg(latency_ms) FILTER (WHERE ok))::float8 AS avg_ms
        FROM check_results
        WHERE monitor_id = $1 AND tenant_id = $2 AND at > now() - make_interval(secs => $3)
        "#,
        id,
        tenant.id,
        window_s,
    )
    .fetch_one(&mut *tx)
    .await?;

    let series = sqlx::query_as!(
        Point,
        r#"
        SELECT
            time_bucket(make_interval(secs => $3), at) AS "t!",
            count(*) AS "checks!",
            count(*) FILTER (WHERE NOT ok) AS "failures!",
            (avg(latency_ms) FILTER (WHERE ok))::float8 AS avg_ms,
            percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms) FILTER (WHERE ok) AS p95_ms
        FROM check_results
        WHERE monitor_id = $1 AND tenant_id = $2 AND at > now() - make_interval(secs => $4)
        GROUP BY 1
        ORDER BY 1
        "#,
        id,
        tenant.id,
        bucket_s,
        window_s,
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    let (checks, failures) = match range {
        Range::Day => counts.day,
        Range::Week => counts.week,
        Range::Month => counts.month,
    };
    Ok(Json(Metrics {
        range: range.name(),
        bucket_s: range.bucket_s(),
        checks,
        failures,
        uptime: Uptimes {
            day: uptime_percent(counts.day.0, counts.day.1),
            week: uptime_percent(counts.week.0, counts.week.1),
            month: uptime_percent(counts.month.0, counts.month.1),
        },
        latency,
        series,
    }))
}

/// Checks and failures of the last 24 hours, 7 days and 30 days of `monitor` as of `now`.
async fn uptime_counts(
    conn: &mut PgConnection,
    tenant: Uuid,
    monitor: Uuid,
    now: DateTime<Utc>,
) -> Result<Counts, sqlx::Error> {
    let day = now - TimeDelta::hours(24);
    let week = Window::last(now, 7);
    let month = Window::last(now, 30);
    // Raw results of the last 24 hours and of the partial first day of each window.
    let raw = sqlx::query!(
        r#"
        SELECT
            count(*) FILTER (WHERE w = 1) AS "checks_day!",
            count(*) FILTER (WHERE w = 1 AND NOT ok) AS "failures_day!",
            count(*) FILTER (WHERE w = 7) AS "checks_week!",
            count(*) FILTER (WHERE w = 7 AND NOT ok) AS "failures_week!",
            count(*) FILTER (WHERE w = 30) AS "checks_month!",
            count(*) FILTER (WHERE w = 30 AND NOT ok) AS "failures_month!"
        FROM (
            SELECT 1 AS w, ok FROM check_results
            WHERE monitor_id = $1 AND tenant_id = $2 AND at > $3
            UNION ALL
            SELECT 7, ok FROM check_results
            WHERE monitor_id = $1 AND tenant_id = $2 AND at > $4 AND at < $5
            UNION ALL
            SELECT 30, ok FROM check_results
            WHERE monitor_id = $1 AND tenant_id = $2 AND at > $6 AND at < $7
        ) AS r
        "#,
        monitor,
        tenant,
        day,
        week.start,
        week.whole_days,
        month.start,
        month.whole_days,
    )
    .fetch_one(&mut *conn)
    .await?;
    // Whole days from the daily aggregate; today comes from its real-time part.
    let daily = sqlx::query!(
        r#"
        SELECT
            coalesce(sum(checks) FILTER (WHERE day >= $3), 0)::bigint AS "checks_week!",
            coalesce(sum(failures) FILTER (WHERE day >= $3), 0)::bigint AS "failures_week!",
            coalesce(sum(checks), 0)::bigint AS "checks_month!",
            coalesce(sum(failures), 0)::bigint AS "failures_month!"
        FROM uptime_daily_of_monitors($1, $2)
        "#,
        month.whole_days,
        &[monitor][..],
        week.whole_days,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(Counts {
        day: (raw.checks_day, raw.failures_day),
        week: (
            raw.checks_week + daily.checks_week,
            raw.failures_week + daily.failures_week,
        ),
        month: (
            raw.checks_month + daily.checks_month,
            raw.failures_month + daily.failures_month,
        ),
    })
}

/// A window of whole days that ends now, split where `uptime_daily` can take over: the raw
/// results from `start` to `whole_days`, the daily aggregate from `whole_days` on.
struct Window {
    /// Now minus the length of the window.
    start: DateTime<Utc>,
    /// The first midnight (UTC, like the aggregate's buckets) after `start`.
    whole_days: DateTime<Utc>,
}

impl Window {
    fn last(now: DateTime<Utc>, days: u64) -> Self {
        let start = now.checked_sub_days(Days::new(days)).unwrap_or(now);
        let whole_days = start
            .date_naive()
            .and_time(NaiveTime::MIN)
            .and_utc()
            .checked_add_days(Days::new(1))
            .unwrap_or(now);
        Self { start, whole_days }
    }
}

/// Checks and failures of the three uptime windows.
struct Counts {
    day: (i64, i64),
    week: (i64, i64),
    month: (i64, i64),
}
