//! `/h/{token}` and `/h/{token}/fail`: public ping endpoints of heartbeat monitors (F-03).
//!
//! A cron job calls its secret URL (`GET`, `HEAD` or `POST`) when it finishes; `/fail`
//! reports that it ran but failed. Each ping updates `last_ping_at` and is published as a
//! `checks.completed` result, so it flows through the same pipeline as any other check. The
//! scheduler reports the missing pings. No tenant header: the token identifies the monitor.

// Axum handlers receive their state by value even when they only borrow it.
#![allow(clippy::needless_pass_by_value)]

use axum::Router;
use axum::extract::State;
use axum::routing::get;
use serde::Serialize;
use taut_http::{AppError, Json, Path, current_trace_id};
use taut_model::events::{CheckCompleted, subjects};

use crate::AppState;

/// Routes under `/h`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/h/{token}", get(ping).post(ping))
        .route("/h/{token}/fail", get(fail).post(fail))
}

#[derive(Debug, Serialize)]
struct PingResponse {
    /// `recorded`, or `paused` when the monitor ignores pings.
    status: &'static str,
}

async fn ping(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<PingResponse>, AppError> {
    record(&state, &token, true).await
}

async fn fail(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<PingResponse>, AppError> {
    record(&state, &token, false).await
}

async fn record(state: &AppState, token: &str, ok: bool) -> Result<Json<PingResponse>, AppError> {
    let pinged = sqlx::query!(
        r#"
        UPDATE monitors SET last_ping_at = now()
        WHERE heartbeat_token = $1 AND kind = 'heartbeat' AND active
        RETURNING id, tenant_id, last_ping_at AS "last_ping_at!"
        "#,
        token
    )
    .fetch_optional(&state.pool)
    .await?;
    let Some(monitor) = pinged else {
        let active = sqlx::query_scalar!(
            "SELECT active FROM monitors WHERE heartbeat_token = $1",
            token
        )
        .fetch_optional(&state.pool)
        .await?;
        return match active {
            Some(false) => Ok(Json(PingResponse { status: "paused" })),
            _ => Err(AppError::NotFound("unknown heartbeat".to_owned())),
        };
    };

    let result = CheckCompleted {
        trace_id: current_trace_id().unwrap_or_else(|| ulid::Ulid::generate().to_string()),
        monitor_id: monitor.id,
        tenant_id: monitor.tenant_id,
        at: monitor.last_ping_at,
        region: state.settings.checks.region.clone(),
        ok,
        status: None,
        latency_ms: None,
        error: (!ok).then(|| "job reported failure".to_owned()),
    };
    state
        .bus
        .publish(subjects::CHECKS_COMPLETED, &result.trace_id, &result)
        .await
        .map_err(|err| {
            tracing::warn!(error = %err, monitor_id = %monitor.id, "heartbeat not published");
            AppError::Unavailable("the ping could not be recorded; retry".to_owned())
        })?;
    tracing::info!(monitor_id = %monitor.id, tenant_id = %monitor.tenant_id, ok, "heartbeat ping");
    Ok(Json(PingResponse { status: "recorded" }))
}
