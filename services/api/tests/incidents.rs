//! `/v1/incidents` and the monitor status derived from the incident service's state.

mod common;

use http::{Method, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn lists_incidents_and_shows_deliveries(pool: PgPool) {
    let app = common::app(pool.clone()).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let monitor = common::create_monitor(
        &app,
        &tenant,
        json!({ "name": "API", "type": "http", "target": "https://example.com" }),
    )
    .await;
    let monitor_id: Uuid = monitor["id"].as_str().unwrap().parse().unwrap();
    let tenant_id: Uuid = tenant.parse().unwrap();

    let resolved = Uuid::now_v7();
    let open = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO incidents (id, tenant_id, monitor_id, started_at, resolved_at, cause) VALUES
         ($1, $3, $4, now() - interval '2 hours', now() - interval '1 hour', 'timeout after 10 s'),
         ($2, $3, $4, now() - interval '5 minutes', NULL, 'connection refused')",
    )
    .bind(resolved)
    .bind(open)
    .bind(tenant_id)
    .bind(monitor_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO monitor_status (monitor_id, tenant_id, status) VALUES ($1, $2, 'down')",
    )
    .bind(monitor_id)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap();
    let channel = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO alert_channels (id, tenant_id, name, kind, config)
         VALUES ($1, $2, 'Ops', 'email', '{\"to\": \"ops@acme.test\"}')",
    )
    .bind(channel)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO notification_deliveries (incident_id, channel_id, event, tenant_id, status)
         VALUES ($1, $2, 'opened', $3, 'sent')",
    )
    .bind(open)
    .bind(channel)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    let (status, all) = common::send(&app, Method::GET, "/v1/incidents", Some(&tenant), None).await;
    assert_eq!(status, StatusCode::OK);
    let all = all.as_array().unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0]["id"], open.to_string(), "open incidents first");
    assert_eq!(all[0]["monitor_name"], "API");
    assert_eq!(all[1]["duration_s"], 3600);

    let (_, only_open) = common::send(
        &app,
        Method::GET,
        "/v1/incidents?status=open",
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(only_open.as_array().unwrap().len(), 1);

    let (_, detail) = common::send(
        &app,
        Method::GET,
        &format!("/v1/incidents/{open}"),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(detail["cause"], "connection refused");
    assert_eq!(detail["deliveries"][0]["channel_type"], "email");
    assert_eq!(detail["deliveries"][0]["status"], "sent");

    let (_, shown) = common::send(
        &app,
        Method::GET,
        &format!("/v1/monitors/{monitor_id}"),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(shown["status"], "down");

    let other = common::create_tenant(&app, "globex").await;
    let (_, none) = common::send(&app, Method::GET, "/v1/incidents", Some(&other), None).await;
    assert_eq!(none, json!([]));
    let (status, _) = common::send(
        &app,
        Method::GET,
        &format!("/v1/incidents/{open}"),
        Some(&other),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
