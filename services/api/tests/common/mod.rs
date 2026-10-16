//! Helpers shared by the integration tests.
//!
//! Every test gets a [`TestApp`]: the router, the key pair that stands in for the auth
//! service, and a signed-in user whose token every request carries. Nothing here talks to
//! the real auth service; tokens are minted locally and verified against the same key.

#![allow(dead_code)] // each test binary uses a different subset

use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use chrono::Utc;
use http::{Method, Request, Response, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use taut_bus::Bus;
use taut_config::Settings;
use taut_identity::{Claims, KeyPair, TokenKind, Verifier};
use tower::ServiceExt;
use uuid::Uuid;

/// Issuer the tests sign with; the same the default settings expect.
const ISSUER: &str = "taut-auth";

/// The api under test plus a signed-in user.
pub struct TestApp {
    /// The router.
    pub router: Router,
    /// Signs the tokens the router accepts.
    pub keys: KeyPair,
    /// Database the router uses, to insert users and memberships directly.
    pub pool: PgPool,
    /// The user whose token `send` attaches.
    pub user_id: Uuid,
    /// Access token of `user_id`.
    pub token: String,
}

impl Deref for TestApp {
    type Target = Router;

    fn deref(&self) -> &Router {
        &self.router
    }
}

impl TestApp {
    /// The same router, acting as `user_id`.
    pub fn as_user(&self, user_id: Uuid) -> Self {
        Self {
            router: self.router.clone(),
            keys: self.keys.clone(),
            pool: self.pool.clone(),
            user_id,
            token: token_for(&self.keys, user_id),
        }
    }
}

/// Router whose pool and bus point at closed ports: `/ready` fails fast, routes that need
/// neither work.
pub async fn app_without_database() -> Router {
    let settings = Arc::new(Settings::default());
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(500))
        .connect_lazy("postgres://taut:taut@127.0.0.1:1/taut")
        .expect("a lazy pool never fails to build");
    let bus = Bus::lazy("nats://127.0.0.1:1")
        .await
        .expect("a lazy bus never fails to build");
    build(settings, pool, bus, &KeyPair::generate())
}

/// App backed by the given pool and the NATS of `just up`, with a signed-in user.
pub async fn app_with(settings: Arc<Settings>, pool: PgPool) -> TestApp {
    let keys = KeyPair::generate();
    let bus = bus().await;
    let router = build(settings, pool.clone(), bus, &keys);
    let user_id = insert_user(&pool, "owner@test.taut").await;
    TestApp {
        router,
        token: token_for(&keys, user_id),
        keys,
        pool,
        user_id,
    }
}

/// App backed by `pool` with the default settings, and a signed-in user.
pub async fn app(pool: PgPool) -> TestApp {
    app_with(Arc::new(Settings::default()), pool).await
}

/// Connection to the NATS of `just up` (`NATS_URL` from the environment or `.env`).
pub async fn bus() -> Bus {
    let settings = Settings::load().expect("settings load");
    Bus::connect(&settings.nats, "api-tests")
        .await
        .expect("nats is up (just up)")
}

fn build(settings: Arc<Settings>, pool: PgPool, bus: Bus, keys: &KeyPair) -> Router {
    let telemetry = taut_telemetry::init(taut_api::SERVICE_NAME, &settings.telemetry)
        .expect("telemetry installs once per process");
    let verifier = Arc::new(Verifier::from_jwks(&keys.jwks(), ISSUER));
    taut_api::app(settings, pool, bus, verifier, telemetry.metrics().clone())
}

/// Inserts a verified user and returns its id.
pub async fn insert_user(pool: &PgPool, email: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO users (id, email, name, password_hash, email_verified_at)
         VALUES ($1, $2, $3, 'unused', now())",
    )
    .bind(id)
    .bind(email)
    .bind(email.split('@').next().unwrap_or("user"))
    .execute(pool)
    .await
    .expect("user inserted");
    id
}

/// Makes `user_id` a member of `tenant` with `role` (`owner` or `member`).
pub async fn add_member(pool: &PgPool, tenant: &str, user_id: Uuid, role: &str) {
    sqlx::query("INSERT INTO memberships (user_id, tenant_id, role) VALUES ($1, $2::uuid, $3)")
        .bind(user_id)
        .bind(tenant)
        .bind(role)
        .execute(pool)
        .await
        .expect("membership inserted");
}

/// An access token for `user_id`, valid 15 minutes, signed with `keys`.
pub fn token_for(keys: &KeyPair, user_id: Uuid) -> String {
    token_with(keys, user_id, 900)
}

/// An access token for `user_id` that expires `ttl_secs` from now (negative: already expired).
pub fn token_with(keys: &KeyPair, user_id: Uuid, ttl_secs: i64) -> String {
    let now = Utc::now().timestamp();
    keys.sign(&Claims {
        iss: ISSUER.to_owned(),
        sub: user_id,
        jti: ulid::Ulid::generate().to_string(),
        sid: Uuid::now_v7(),
        iat: now,
        exp: now + ttl_secs,
        auth_time: now,
        mfa: false,
        ver: 1,
        kind: TokenKind::Access,
    })
    .expect("token signs")
}

/// Sends a request as the app's user, with an optional JSON body and tenant header; returns
/// status and JSON body (`Value::Null` when the body is empty).
pub async fn send(
    app: &TestApp,
    method: Method,
    path: &str,
    tenant: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    send_raw(&app.router, method, path, Some(&app.token), tenant, body).await
}

/// Like [`send`], with an explicit bearer token (or none).
pub async fn send_raw(
    router: &Router,
    method: Method,
    path: &str,
    token: Option<&str>,
    tenant: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    if let Some(tenant) = tenant {
        request = request.header("x-tenant-id", tenant);
    }
    let request = match body {
        Some(body) => request
            .header("content-type", "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    }
    .expect("valid request");
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("router answers");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body is readable")
        .to_bytes();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("body is JSON")
    };
    (status, body)
}

/// Creates a tenant owned by the app's user and returns its id.
pub async fn create_tenant(app: &TestApp, slug: &str) -> String {
    let (status, body) = send(
        app,
        Method::POST,
        "/v1/tenants",
        None,
        Some(json!({ "name": format!("Tenant {slug}"), "slug": slug })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["id"].as_str().expect("tenant id").to_owned()
}

/// Creates a monitor for `tenant` and returns the response body.
pub async fn create_monitor(app: &TestApp, tenant: &str, body: Value) -> Value {
    let (status, body) = send(app, Method::POST, "/v1/monitors", Some(tenant), Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body
}

/// Builds a `GET` request with an empty body.
pub fn get(path: &str) -> Request<Body> {
    Request::get(path)
        .body(Body::empty())
        .expect("valid request")
}

/// Reads a JSON response body.
pub async fn json(response: Response<Body>) -> serde_json::Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body is readable")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("body is JSON")
}

/// Reads a text response body.
pub async fn text(response: Response<Body>) -> String {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body is readable")
        .to_bytes();
    String::from_utf8(bytes.to_vec()).expect("body is UTF-8")
}
