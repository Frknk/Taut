//! Notifier against real PostgreSQL, a local webhook receiver and Mailpit (`just up`).

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use taut_bus::Handled;
use taut_config::Settings;
use taut_model::events::{IncidentChanged, TestNotification};
use taut_notifier::Notifier;
use taut_notifier::send::Senders;
use tokio::net::TcpListener;
use uuid::Uuid;

type Received = Arc<Mutex<Vec<(String, Value)>>>;

/// Webhook receiver: `/ok/*` answers 204 and records the body, `/fail` answers 500.
async fn receiver() -> (SocketAddr, Received) {
    let received: Received = Arc::default();
    let app = Router::new()
        .route(
            "/ok/{name}",
            post(
                |State(received): State<Received>,
                 axum::extract::Path(name): axum::extract::Path<String>,
                 axum::Json(body): axum::Json<Value>| async move {
                    received.lock().unwrap().push((name, body));
                    StatusCode::NO_CONTENT
                },
            ),
        )
        .route(
            "/fail",
            post(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
        )
        .with_state(Arc::clone(&received));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (address, received)
}

fn notifier(pool: &PgPool) -> Notifier {
    let settings = Settings::load().expect("settings");
    let senders = Senders::new(&settings.notifier, true).expect("senders");
    Notifier::new(pool.clone(), senders, "http://panel.test")
}

struct Fixture {
    tenant: Uuid,
    monitor: Uuid,
    incident: Uuid,
}

async fn fixture(pool: &PgPool) -> Fixture {
    let tenant = Uuid::now_v7();
    let monitor = Uuid::now_v7();
    let incident = Uuid::now_v7();
    sqlx::query("INSERT INTO tenants (id, slug, name) VALUES ($1, $2, 'Acme')")
        .bind(tenant)
        .bind(format!("t-{}", tenant.simple()))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO monitors (id, tenant_id, name, kind, target)
         VALUES ($1, $2, 'API', 'http', 'https://example.com/')",
    )
    .bind(monitor)
    .bind(tenant)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO incidents (id, tenant_id, monitor_id, started_at, cause)
         VALUES ($1, $2, $3, now(), 'status 503 (expected 2xx)')",
    )
    .bind(incident)
    .bind(tenant)
    .bind(monitor)
    .execute(pool)
    .await
    .unwrap();
    Fixture {
        tenant,
        monitor,
        incident,
    }
}

async fn channel(pool: &PgPool, tenant: Uuid, kind: &str, config: Value) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO alert_channels (id, tenant_id, name, kind, config) VALUES ($1, $2, $3, $3, $4)",
    )
    .bind(id)
    .bind(tenant)
    .bind(kind)
    .bind(config)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn opened(fixture: &Fixture) -> IncidentChanged {
    IncidentChanged {
        trace_id: "trace".to_owned(),
        incident_id: fixture.incident,
        tenant_id: fixture.tenant,
        monitor_id: fixture.monitor,
        monitor_name: "API".to_owned(),
        started_at: Utc::now(),
        resolved_at: None,
        cause: "status 503 (expected 2xx)".to_owned(),
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn webhooks_get_one_message_per_transition(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let (server, received) = receiver().await;
    channel(
        &pool,
        fixture.tenant,
        "discord",
        json!({ "url": format!("http://{server}/ok/discord") }),
    )
    .await;
    channel(
        &pool,
        fixture.tenant,
        "webhook",
        json!({ "url": format!("http://{server}/ok/webhook") }),
    )
    .await;
    let notifier = notifier(&pool);
    let event = opened(&fixture);

    assert!(matches!(notifier.incident(&event).await, Handled::Done));
    // A redelivered event does not reach the channels again.
    assert!(matches!(notifier.incident(&event).await, Handled::Done));

    let received = received.lock().unwrap().clone();
    assert_eq!(received.len(), 2, "{received:?}");
    let discord = &received
        .iter()
        .find(|(name, _)| name == "discord")
        .unwrap()
        .1;
    assert!(
        discord["content"]
            .as_str()
            .unwrap()
            .contains("**API** está caído")
    );
    let webhook = &received
        .iter()
        .find(|(name, _)| name == "webhook")
        .unwrap()
        .1;
    assert_eq!(webhook["event"], "incident.opened");
    assert_eq!(webhook["monitor"]["name"], "API");

    let sent: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notification_deliveries WHERE status = 'sent' AND event = 'opened'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(sent, 2);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn failing_channels_are_retried_and_reported(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let (server, received) = receiver().await;
    let good = channel(
        &pool,
        fixture.tenant,
        "slack",
        json!({ "url": format!("http://{server}/ok/slack") }),
    )
    .await;
    let bad = channel(
        &pool,
        fixture.tenant,
        "webhook",
        json!({ "url": format!("http://{server}/fail") }),
    )
    .await;
    let notifier = notifier(&pool);
    let event = opened(&fixture);

    assert!(matches!(notifier.incident(&event).await, Handled::Retry(_)));
    assert!(matches!(notifier.incident(&event).await, Handled::Retry(_)));
    assert_eq!(
        received.lock().unwrap().len(),
        1,
        "the healthy channel is not resent"
    );

    let (status, attempts, error): (String, i32, Option<String>) = sqlx::query_as(
        "SELECT status, attempts, last_error FROM notification_deliveries WHERE channel_id = $1",
    )
    .bind(bad)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "failed");
    assert_eq!(attempts, 2);
    assert_eq!(
        error.as_deref(),
        Some("webhook answered 500 Internal Server Error")
    );
    let channel_error: Option<String> =
        sqlx::query_scalar("SELECT last_delivery_error FROM alert_channels WHERE id = $1")
            .bind(bad)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(channel_error.is_some());
    let good_error: Option<String> =
        sqlx::query_scalar("SELECT last_delivery_error FROM alert_channels WHERE id = $1")
            .bind(good)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(good_error, None);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn email_alerts_reach_the_smtp_server(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let to = format!("ops-{}@taut.test", fixture.incident.simple());
    channel(&pool, fixture.tenant, "email", json!({ "to": to })).await;
    let mut event = opened(&fixture);
    event.resolved_at = Some(event.started_at + chrono::Duration::seconds(90));

    assert!(matches!(
        notifier(&pool).incident(&event).await,
        Handled::Done
    ));

    // Mailpit stores the message; its API finds it by recipient.
    let search = format!("http://localhost:8025/api/v1/search?query=to:{to}");
    let found: Value = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let found: Value = reqwest::get(&search).await.unwrap().json().await.unwrap();
            if found["messages_count"].as_u64() == Some(1) {
                return found;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("email delivered to mailpit");
    assert_eq!(found["messages"][0]["Subject"], "[Taut] Recuperado: API");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn test_requests_send_one_message_and_record_the_outcome(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let (server, received) = receiver().await;
    let id = channel(
        &pool,
        fixture.tenant,
        "discord",
        json!({ "url": format!("http://{server}/ok/discord") }),
    )
    .await;
    let request = TestNotification {
        trace_id: "trace".to_owned(),
        tenant_id: fixture.tenant,
        channel_id: id,
    };
    assert!(matches!(
        notifier(&pool).test(&request).await,
        Handled::Done
    ));
    let received = received.lock().unwrap().clone();
    assert!(
        received[0].1["content"]
            .as_str()
            .unwrap()
            .contains("prueba")
    );
    let delivered: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT last_delivery_at FROM alert_channels WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(delivered.is_some());

    let unknown = TestNotification {
        channel_id: Uuid::now_v7(),
        ..request
    };
    assert!(matches!(
        notifier(&pool).test(&unknown).await,
        Handled::Drop(_)
    ));
}
