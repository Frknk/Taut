//! Heartbeat pings against real PostgreSQL and NATS (`just up`).

mod common;

use std::time::Duration;

use futures::StreamExt;
use http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use taut_model::events::{CheckCompleted, subjects};
use uuid::Uuid;

async fn next_result_for(
    subscriber: &mut async_nats::Subscriber,
    monitor_id: Uuid,
) -> CheckCompleted {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = subscriber.next().await {
            let event: CheckCompleted = serde_json::from_slice(&message.payload).unwrap();
            if event.monitor_id == monitor_id {
                return event;
            }
        }
        panic!("subscription closed");
    })
    .await
    .expect("result published")
}

async fn heartbeat(app: &axum::Router) -> (String, Value) {
    let tenant = common::create_tenant(app, "acme").await;
    let monitor = common::create_monitor(
        app,
        &tenant,
        json!({ "name": "Backup", "type": "heartbeat" }),
    )
    .await;
    (tenant, monitor)
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn ping_records_and_publishes_a_healthy_result(pool: PgPool) {
    let app = common::app(pool).await;
    let (tenant, monitor) = heartbeat(&app).await;
    let id: Uuid = monitor["id"].as_str().unwrap().parse().unwrap();
    let token = monitor["heartbeat_token"].as_str().unwrap();
    let mut results = common::bus()
        .await
        .client()
        .subscribe(subjects::CHECKS_COMPLETED)
        .await
        .unwrap();

    let (status, body) = common::send(&app, Method::POST, &format!("/h/{token}"), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "recorded");

    let event = next_result_for(&mut results, id).await;
    assert!(event.ok);
    assert_eq!(event.tenant_id.to_string(), tenant);
    assert_eq!(
        event.trace_id.len(),
        26,
        "the request id becomes the trace id"
    );

    let (_, shown) = common::send(
        &app,
        Method::GET,
        &format!("/v1/monitors/{id}"),
        Some(&tenant),
        None,
    )
    .await;
    assert!(shown["last_ping_at"].is_string());
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn fail_ping_publishes_a_failure(pool: PgPool) {
    let app = common::app(pool).await;
    let (_, monitor) = heartbeat(&app).await;
    let id: Uuid = monitor["id"].as_str().unwrap().parse().unwrap();
    let token = monitor["heartbeat_token"].as_str().unwrap();
    let mut results = common::bus()
        .await
        .client()
        .subscribe(subjects::CHECKS_COMPLETED)
        .await
        .unwrap();

    let (status, _) =
        common::send(&app, Method::GET, &format!("/h/{token}/fail"), None, None).await;
    assert_eq!(status, StatusCode::OK);
    let event = next_result_for(&mut results, id).await;
    assert!(!event.ok);
    assert_eq!(event.error.as_deref(), Some("job reported failure"));
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn unknown_and_paused_heartbeats(pool: PgPool) {
    let app = common::app(pool).await;
    let (status, body) = common::send(&app, Method::GET, "/h/nope", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");

    let (tenant, monitor) = heartbeat(&app).await;
    let id = monitor["id"].as_str().unwrap();
    common::send(
        &app,
        Method::POST,
        &format!("/v1/monitors/{id}/pause"),
        Some(&tenant),
        None,
    )
    .await;
    let token = monitor["heartbeat_token"].as_str().unwrap();
    let (status, body) = common::send(&app, Method::GET, &format!("/h/{token}"), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "paused");
}
