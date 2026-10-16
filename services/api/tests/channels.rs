//! `/v1/alert-channels` against real PostgreSQL and NATS.

mod common;

use std::time::Duration;

use futures::StreamExt;
use http::{Method, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use taut_model::events::{TestNotification, subjects};

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn channels_crud(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;

    let (status, created) = common::send(
        &app,
        Method::POST,
        "/v1/alert-channels",
        Some(&tenant),
        Some(json!({ "name": "Guardia", "type": "email", "config": { "to": "ops@acme.test" } })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["type"], "email");
    assert_eq!(created["active"], true);
    assert_eq!(created["last_delivery_at"], serde_json::Value::Null);
    let path = format!("/v1/alert-channels/{}", created["id"].as_str().unwrap());

    let (status, body) = common::send(
        &app,
        Method::POST,
        "/v1/alert-channels",
        Some(&tenant),
        Some(json!({ "name": "Mal", "type": "discord", "config": { "url": "nope" } })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let (status, updated) = common::send(
        &app,
        Method::PUT,
        &path,
        Some(&tenant),
        Some(json!({
            "name": "Discord",
            "type": "discord",
            "config": { "url": "https://discord.com/api/webhooks/1/abc" },
            "active": false,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["type"], "discord");
    assert_eq!(updated["active"], false);

    let other = common::create_tenant(&app, "globex").await;
    let (status, _) = common::send(&app, Method::GET, &path, Some(&other), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, list) =
        common::send(&app, Method::GET, "/v1/alert-channels", Some(&tenant), None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (status, _) = common::send(&app, Method::DELETE, &path, Some(&tenant), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn test_button_queues_a_notification(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let (_, channel) = common::send(
        &app,
        Method::POST,
        "/v1/alert-channels",
        Some(&tenant),
        Some(json!({ "name": "Ops", "type": "slack", "config": { "url": "https://hooks.slack.com/x" } })),
    )
    .await;
    let id = channel["id"].as_str().unwrap();
    let mut tests = common::bus()
        .await
        .client()
        .subscribe(subjects::NOTIFICATIONS_TEST)
        .await
        .unwrap();

    let (status, body) = common::send(
        &app,
        Method::POST,
        &format!("/v1/alert-channels/{id}/test"),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["status"], "queued");

    let request = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = tests.next().await {
            let request: TestNotification = serde_json::from_slice(&message.payload).unwrap();
            if request.channel_id.to_string() == id {
                return request;
            }
        }
        panic!("subscription closed");
    })
    .await
    .expect("test request published");
    assert_eq!(request.tenant_id.to_string(), tenant);
}
