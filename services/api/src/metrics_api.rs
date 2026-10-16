//! `GET /v1/monitors/{id}/metrics`: latency and uptime of a monitor (F-26).
//!
//! Read straight from the `check_results` hypertable with `time_bucket` and
//! `percentile_cont`; the CQRS split moves these reads to their own service in week 10.
//! Latency only counts healthy checks, so timeouts do not drown the percentiles.

// Axum handlers receive their state by value even when they only borrow it.
#![allow(clippy::needless_pass_by_value)]

use axum::Router;
use axum::extract::State;
use axum::routing::get;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
    CurrentTenant(tenant): CurrentTenant,
    Path(id): Path<Uuid>,
    Query(query): Query<MetricsQuery>,
) -> Result<Json<Metrics>, AppError> {
    if monitors::find(&state.pool, tenant.id, id).await?.is_none() {
        return Err(AppError::NotFound("monitor not found".to_owned()));
    }
    let range = query.range;
    #[allow(clippy::cast_precision_loss)] // a few million seconds at most
    let (window_s, bucket_s) = (range.window_s() as f64, range.bucket_s() as f64);

    let counts = sqlx::query!(
        r#"
        SELECT
            count(*) FILTER (WHERE at > now() - interval '24 hours') AS "checks_day!",
            count(*) FILTER (WHERE at > now() - interval '24 hours' AND NOT ok) AS "failures_day!",
            count(*) FILTER (WHERE at > now() - interval '7 days') AS "checks_week!",
            count(*) FILTER (WHERE at > now() - interval '7 days' AND NOT ok) AS "failures_week!",
            count(*) AS "checks_month!",
            count(*) FILTER (WHERE NOT ok) AS "failures_month!",
            count(*) FILTER (WHERE at > now() - make_interval(secs => $3)) AS "checks_range!",
            count(*) FILTER (WHERE at > now() - make_interval(secs => $3) AND NOT ok)
                AS "failures_range!"
        FROM check_results
        WHERE monitor_id = $1 AND tenant_id = $2 AND at > now() - interval '30 days'
        "#,
        id,
        tenant.id,
        window_s,
    )
    .fetch_one(&state.pool)
    .await?;

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
    .fetch_one(&state.pool)
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
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(Metrics {
        range: range.name(),
        bucket_s: range.bucket_s(),
        checks: counts.checks_range,
        failures: counts.failures_range,
        uptime: Uptimes {
            day: uptime_percent(counts.checks_day, counts.failures_day),
            week: uptime_percent(counts.checks_week, counts.failures_week),
            month: uptime_percent(counts.checks_month, counts.failures_month),
        },
        latency,
        series,
    }))
}
