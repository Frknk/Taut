//! Checks against a local echo server; no NATS or database needed.

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::Redirect;
use axum::routing::get;
use chrono::Utc;
use taut_checker::checks::INTERNAL_ADDRESS;
use taut_checker::{Checker, execute};
use taut_model::MonitorKind;
use taut_model::events::CheckRequested;
use tokio::net::TcpListener;
use uuid::Uuid;

async fn echo_server() -> SocketAddr {
    let app = Router::new()
        .route("/ok", get(|| async { "Welcome to taut" }))
        .route(
            "/status/{code}",
            get(|Path(code): Path<u16>| async move {
                StatusCode::from_u16(code).expect("valid status")
            }),
        )
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(3)).await;
                "late"
            }),
        )
        .route("/to-ok", get(|| async { Redirect::temporary("/ok") }));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    address
}

fn request(kind: MonitorKind, target: String) -> CheckRequested {
    let now = Utc::now();
    CheckRequested {
        trace_id: "test".to_owned(),
        monitor_id: Uuid::now_v7(),
        tenant_id: Uuid::now_v7(),
        kind,
        target,
        timeout_s: 2,
        expected_status: None,
        keyword: None,
        scheduled_at: now,
        expires_at: now + chrono::Duration::seconds(60),
    }
}

fn checker() -> Checker {
    Checker::new(true).expect("checker builds")
}

#[tokio::test]
async fn healthy_http_target_is_up() {
    let server = echo_server().await;
    let outcome = checker()
        .run(&request(MonitorKind::Http, format!("http://{server}/ok")))
        .await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(outcome.status, Some(200));
    assert_eq!(outcome.error, None);
}

#[tokio::test]
async fn unexpected_status_is_a_failure_with_cause() {
    let server = echo_server().await;
    let outcome = checker()
        .run(&request(
            MonitorKind::Http,
            format!("http://{server}/status/500"),
        ))
        .await;
    assert!(!outcome.ok);
    assert_eq!(outcome.status, Some(500));
    assert_eq!(outcome.error.as_deref(), Some("status 500 (expected 2xx)"));
}

#[tokio::test]
async fn expected_status_replaces_the_2xx_rule() {
    let server = echo_server().await;
    let mut no_content = request(MonitorKind::Http, format!("http://{server}/status/204"));
    no_content.expected_status = Some(204);
    assert!(checker().run(&no_content).await.ok);

    let mut wrong = request(MonitorKind::Http, format!("http://{server}/ok"));
    wrong.expected_status = Some(204);
    let outcome = checker().run(&wrong).await;
    assert_eq!(outcome.error.as_deref(), Some("status 200 (expected 204)"));
}

#[tokio::test]
async fn keyword_must_appear_in_the_body() {
    let server = echo_server().await;
    let mut present = request(MonitorKind::Keyword, format!("http://{server}/ok"));
    present.keyword = Some("Welcome".to_owned());
    assert!(checker().run(&present).await.ok);

    let mut missing = request(MonitorKind::Keyword, format!("http://{server}/ok"));
    missing.keyword = Some("Goodbye".to_owned());
    let outcome = checker().run(&missing).await;
    assert!(!outcome.ok);
    assert_eq!(outcome.status, Some(200));
    assert_eq!(outcome.error.as_deref(), Some("keyword missing"));
}

#[tokio::test]
async fn slow_target_times_out() {
    let server = echo_server().await;
    let mut slow = request(MonitorKind::Http, format!("http://{server}/slow"));
    slow.timeout_s = 1;
    let outcome = checker().run(&slow).await;
    assert!(!outcome.ok);
    assert_eq!(outcome.error.as_deref(), Some("timeout after 1 s"));
    assert_eq!(outcome.latency_ms, 1000);
}

#[tokio::test]
async fn closed_port_is_connection_refused() {
    let http = checker()
        .run(&request(
            MonitorKind::Http,
            "http://127.0.0.1:1/".to_owned(),
        ))
        .await;
    assert_eq!(http.error.as_deref(), Some("connection refused"));

    let tcp = checker()
        .run(&request(MonitorKind::Tcp, "127.0.0.1:1".to_owned()))
        .await;
    assert_eq!(tcp.error.as_deref(), Some("connection refused"));
}

#[tokio::test]
async fn unknown_host_is_a_dns_failure() {
    let outcome = checker()
        .run(&request(
            MonitorKind::Http,
            "http://does-not-exist.invalid/".to_owned(),
        ))
        .await;
    assert_eq!(
        outcome.error.as_deref(),
        Some("dns: could not resolve does-not-exist.invalid")
    );
}

#[tokio::test]
async fn open_tcp_port_is_up() {
    let server = echo_server().await;
    let outcome = checker()
        .run(&request(MonitorKind::Tcp, server.to_string()))
        .await;
    assert!(outcome.ok, "{outcome:?}");
}

#[tokio::test]
async fn redirects_are_followed() {
    let server = echo_server().await;
    let outcome = checker()
        .run(&request(
            MonitorKind::Http,
            format!("http://{server}/to-ok"),
        ))
        .await;
    assert!(outcome.ok, "{outcome:?}");
}

#[tokio::test]
async fn internal_targets_are_refused_unless_allowed() {
    let server = echo_server().await;
    let strict = Checker::new(false).expect("checker builds");
    for (kind, target) in [
        (MonitorKind::Http, format!("http://{server}/ok")),
        (
            MonitorKind::Http,
            format!("http://localhost:{}/ok", server.port()),
        ),
        (MonitorKind::Tcp, server.to_string()),
        (MonitorKind::Tcp, format!("localhost:{}", server.port())),
    ] {
        let outcome = strict.run(&request(kind, target.clone())).await;
        assert_eq!(outcome.error.as_deref(), Some(INTERNAL_ADDRESS), "{target}");
    }
}

#[tokio::test]
async fn expired_requests_are_skipped() {
    let mut late = request(MonitorKind::Http, "http://127.0.0.1:1/".to_owned());
    late.expires_at = Utc::now() - chrono::Duration::seconds(1);
    assert!(execute(&checker(), "local", &late).await.is_none());
}

#[tokio::test]
async fn results_carry_the_request_identity() {
    let server = echo_server().await;
    let check = request(MonitorKind::Http, format!("http://{server}/ok"));
    let completed = execute(&checker(), "sa-east-1", &check)
        .await
        .expect("not expired");
    assert_eq!(completed.trace_id, check.trace_id);
    assert_eq!(completed.monitor_id, check.monitor_id);
    assert_eq!(completed.tenant_id, check.tenant_id);
    assert_eq!(completed.region, "sa-east-1");
    assert!(completed.ok);
}
