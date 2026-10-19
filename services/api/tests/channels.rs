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

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn webhook_channels_get_a_signing_secret_once(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;

    let (status, created) = common::send(
        &app,
        Method::POST,
        "/v1/alert-channels",
        Some(&tenant),
        Some(json!({ "name": "Hook", "type": "webhook", "config": { "url": "https://hooks.acme.test/taut" } })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let secret = created["signing_secret"]
        .as_str()
        .expect("secret on create");
    assert!(secret.starts_with("whsec_"), "{secret}");
    assert_eq!(created["signing_secret_set"], true);
    let path = format!("/v1/alert-channels/{}", created["id"].as_str().unwrap());

    // Reads say there is a secret but never show it.
    let (_, shown) = common::send(&app, Method::GET, &path, Some(&tenant), None).await;
    assert_eq!(shown["signing_secret_set"], true);
    assert!(shown.get("signing_secret").is_none(), "{shown}");
    let (_, list) =
        common::send(&app, Method::GET, "/v1/alert-channels", Some(&tenant), None).await;
    assert!(!list.to_string().contains(secret));

    // An update that keeps the kind keeps the secret and does not show it again.
    let (status, updated) = common::send(
        &app,
        Method::PUT,
        &path,
        Some(&tenant),
        Some(json!({ "name": "Hook", "type": "webhook", "config": { "url": "https://hooks.acme.test/v2" }, "active": false })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["signing_secret_set"], true);
    assert!(updated.get("signing_secret").is_none(), "{updated}");
    let stored: Option<String> =
        sqlx::query_scalar("SELECT signing_secret FROM alert_channels WHERE id = $1::uuid")
            .bind(created["id"].as_str().unwrap())
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(stored.as_deref(), Some(secret));

    // Discord and Slack do not verify signatures: no secret.
    let (_, slack) = common::send(
        &app,
        Method::POST,
        "/v1/alert-channels",
        Some(&tenant),
        Some(json!({ "name": "Ops", "type": "slack", "config": { "url": "https://hooks.slack.com/x" } })),
    )
    .await;
    assert!(slack.get("signing_secret").is_none(), "{slack}");
    assert_eq!(slack["signing_secret_set"], false);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn rotating_the_secret_replaces_it(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let (_, created) = common::send(
        &app,
        Method::POST,
        "/v1/alert-channels",
        Some(&tenant),
        Some(json!({ "name": "Hook", "type": "webhook", "config": { "url": "https://hooks.acme.test/taut" } })),
    )
    .await;
    let rotate = format!(
        "/v1/alert-channels/{}/rotate-secret",
        created["id"].as_str().unwrap()
    );

    let (status, rotated) = common::send(&app, Method::POST, &rotate, Some(&tenant), None).await;
    assert_eq!(status, StatusCode::OK, "{rotated}");
    let secret = rotated["signing_secret"].as_str().unwrap();
    assert!(secret.starts_with("whsec_"));
    assert_ne!(secret, created["signing_secret"].as_str().unwrap());

    // Members cannot rotate, and owners need a recent authentication.
    let member_id = common::insert_user(&app.pool, "member@test.taut").await;
    common::add_member(&app.pool, &tenant, member_id, "member").await;
    let (status, _) = common::send(
        &app.as_user(member_id),
        Method::POST,
        &rotate,
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let stale = common::TestApp {
        token: common::token_authenticated_ago(&app.keys, app.user_id, 600),
        ..app.as_user(app.user_id)
    };
    let (status, answer) = common::send(&stale, Method::POST, &rotate, Some(&tenant), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(answer["error"]["code"], "reauth_required");

    // Another tenant does not see the channel; Slack has nothing to rotate.
    let other = common::create_tenant(&app, "globex").await;
    let (status, _) = common::send(&app, Method::POST, &rotate, Some(&other), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, slack) = common::send(
        &app,
        Method::POST,
        "/v1/alert-channels",
        Some(&tenant),
        Some(json!({ "name": "Ops", "type": "slack", "config": { "url": "https://hooks.slack.com/x" } })),
    )
    .await;
    let (status, body) = common::send(
        &app,
        Method::POST,
        &format!(
            "/v1/alert-channels/{}/rotate-secret",
            slack["id"].as_str().unwrap()
        ),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn changing_the_kind_drops_or_creates_the_secret(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let (_, created) = common::send(
        &app,
        Method::POST,
        "/v1/alert-channels",
        Some(&tenant),
        Some(json!({ "name": "Hook", "type": "webhook", "config": { "url": "https://hooks.acme.test/taut" } })),
    )
    .await;
    let path = format!("/v1/alert-channels/{}", created["id"].as_str().unwrap());

    let (status, discord) = common::send(
        &app,
        Method::PUT,
        &path,
        Some(&tenant),
        Some(json!({ "name": "Hook", "type": "discord", "config": { "url": "https://discord.com/api/webhooks/1/abc" } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{discord}");
    assert_eq!(discord["signing_secret_set"], false);

    let (status, webhook) = common::send(
        &app,
        Method::PUT,
        &path,
        Some(&tenant),
        Some(json!({ "name": "Hook", "type": "webhook", "config": { "url": "https://hooks.acme.test/taut" } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{webhook}");
    assert_eq!(webhook["signing_secret_set"], true);
    let secret = webhook["signing_secret"]
        .as_str()
        .expect("new secret shown");
    assert_ne!(secret, created["signing_secret"].as_str().unwrap());
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn updating_an_unsigned_webhook_keeps_it_unsigned(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let (_, created) = common::send(
        &app,
        Method::POST,
        "/v1/alert-channels",
        Some(&tenant),
        Some(json!({ "name": "Hook", "type": "webhook", "config": { "url": "https://hooks.acme.test/taut" } })),
    )
    .await;
    let id = created["id"].as_str().unwrap();
    // A webhook created before signing existed.
    sqlx::query("UPDATE alert_channels SET signing_secret = NULL WHERE id = $1::uuid")
        .bind(id)
        .execute(&app.pool)
        .await
        .unwrap();

    // Muting it from the panel sends the whole channel back; no secret appears behind the
    // owner's back.
    let (status, muted) = common::send(
        &app,
        Method::PUT,
        &format!("/v1/alert-channels/{id}"),
        Some(&tenant),
        Some(json!({ "name": "Hook", "type": "webhook", "config": { "url": "https://hooks.acme.test/taut" }, "active": false })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{muted}");
    assert_eq!(muted["signing_secret_set"], false);
    assert!(muted.get("signing_secret").is_none(), "{muted}");
    let stored: Option<String> =
        sqlx::query_scalar("SELECT signing_secret FROM alert_channels WHERE id = $1::uuid")
            .bind(id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(stored, None);
}
