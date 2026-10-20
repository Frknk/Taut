//! Per-tenant request budgets (needs `just up`: PostgreSQL, NATS and Redis).

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use http::{Method, Request, Response, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use taut_api::ratelimit::{Bucket, window};
use taut_config::Settings;
use tower::ServiceExt;
use uuid::Uuid;

use common::TestApp;

/// `GET /v1/monitors` with `token` as the bearer, keeping the response headers.
async fn list_monitors(app: &TestApp, token: &str, tenant: Option<&str>) -> Response<Body> {
    let mut request =
        Request::get("/v1/monitors").header("authorization", format!("Bearer {token}"));
    if let Some(tenant) = tenant {
        request = request.header("x-tenant-id", tenant);
    }
    app.router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

/// Sets the counter of `bucket` to `count` for this minute and the next, so the assertions
/// hold even if the minute turns while the test runs.
async fn spend(tenant: &str, bucket: Bucket, count: u64) {
    let client = redis::Client::open(common::redis_url()).unwrap();
    let mut redis = client.get_multiplexed_async_connection().await.unwrap();
    let tenant: Uuid = tenant.parse().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    for at in [now, now + 60] {
        let (key, _) = window(tenant, bucket, at);
        redis::cmd("SET")
            .arg(&key)
            .arg(count)
            .arg("EX")
            .arg(180)
            .exec_async(&mut redis)
            .await
            .unwrap();
    }
}

async fn create_key(app: &TestApp, tenant: &str) -> String {
    let (status, body) = common::send(
        app,
        Method::POST,
        "/v1/api-keys",
        Some(tenant),
        Some(json!({ "name": "CI", "scope": "read" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["key"].as_str().unwrap().to_owned()
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn api_keys_get_429_after_the_plan_budget(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let key = create_key(&app, &tenant).await;

    // The 60th request of the minute still goes through.
    spend(&tenant, Bucket::ApiKey, 59).await;
    assert_eq!(
        list_monitors(&app, &key, None).await.status(),
        StatusCode::OK
    );

    // The 61st does not, and says when to come back.
    spend(&tenant, Bucket::ApiKey, 60).await;
    let response = list_monitors(&app, &key, None).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after: u64 = response.headers()["retry-after"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=60).contains(&retry_after), "{retry_after}");
    let body = common::json(response).await;
    assert_eq!(body["error"]["code"], "too_many_requests");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn the_budget_follows_the_plan(pool: PgPool) {
    let app = common::app(pool.clone()).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let key = create_key(&app, &tenant).await;
    sqlx::query("UPDATE tenants SET plan = 'pro' WHERE slug = 'acme'")
        .execute(&pool)
        .await
        .unwrap();

    // Well past the 60 of free, a pro tenant still has room...
    spend(&tenant, Bucket::ApiKey, 299).await;
    assert_eq!(
        list_monitors(&app, &key, None).await.status(),
        StatusCode::OK
    );
    // ...until its own budget of 300 is spent.
    spend(&tenant, Bucket::ApiKey, 300).await;
    assert_eq!(
        list_monitors(&app, &key, None).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn the_panel_and_other_tenants_keep_their_budget(pool: PgPool) {
    let app = common::app(pool).await;
    let acme = common::create_tenant(&app, "acme").await;
    let globex = common::create_tenant(&app, "globex").await;
    let acme_key = create_key(&app, &acme).await;
    let globex_key = create_key(&app, &globex).await;

    spend(&acme, Bucket::ApiKey, 60).await;
    assert_eq!(
        list_monitors(&app, &acme_key, None).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    // The panel spends its own budget, and another tenant's keys theirs.
    assert_eq!(
        list_monitors(&app, &app.token, Some(&acme)).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        list_monitors(&app, &globex_key, None).await.status(),
        StatusCode::OK
    );

    spend(&acme, Bucket::Panel, 600).await;
    assert_eq!(
        list_monitors(&app, &app.token, Some(&acme)).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        list_monitors(&app, &app.token, Some(&globex))
            .await
            .status(),
        StatusCode::OK
    );
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn a_redis_outage_lets_requests_through_quickly(pool: PgPool) {
    let app =
        common::app_with_limiter(Arc::new(Settings::default()), pool, common::closed_redis()).await;
    let tenant = common::create_tenant(&app, "acme").await;

    let started = Instant::now();
    for _ in 0..3 {
        assert_eq!(
            list_monitors(&app, &app.token, Some(&tenant))
                .await
                .status(),
            StatusCode::OK
        );
    }
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}
