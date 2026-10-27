//! Turnstile on registration, login and password recovery, against a local `siteverify`.

mod common;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use common::{Client, client_with, unique_email};
use http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use taut_config::{Environment, Settings};
use tokio::net::TcpListener;

const SECRET: &str = "test-turnstile-secret";
const GOOD: &str = "pass-token";
const PASSWORD: &str = "correct horse battery";

type Seen = Arc<Mutex<Vec<Value>>>;

/// A `siteverify` that accepts only [`GOOD`] with [`SECRET`] and records every request.
async fn siteverify() -> (String, Seen) {
    let seen: Seen = Arc::default();
    let app = Router::new()
        .route(
            "/siteverify",
            post(
                |State(seen): State<Seen>, Json(body): Json<Value>| async move {
                    let success = body["secret"] == SECRET && body["response"] == GOOD;
                    seen.lock().unwrap().push(body);
                    let codes: &[&str] = if success {
                        &[]
                    } else {
                        &["invalid-input-response"]
                    };
                    Json(json!({ "success": success, "error-codes": codes }))
                },
            ),
        )
        .with_state(Arc::clone(&seen));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}/siteverify"), seen)
}

/// A client whose auth service verifies captchas against `url`.
async fn guarded(pool: PgPool, url: &str) -> Client {
    let url = url.to_owned();
    client_with(pool, move |settings| {
        SECRET.clone_into(&mut settings.auth.turnstile_secret);
        settings.auth.turnstile_url = url;
    })
    .await
}

fn code(body: &Value) -> &str {
    body["error"]["code"].as_str().unwrap_or_default()
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn registration_needs_a_valid_captcha(pool: PgPool) {
    let (url, _) = siteverify().await;
    let mut client = guarded(pool, &url).await;
    let email = unique_email("captcha-register");
    let body = json!({ "name": "Ada", "email": email, "password": PASSWORD });

    let reply = client
        .send(Method::POST, "/v1/auth/register", Some(body.clone()))
        .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);
    assert_eq!(code(&reply.body), "captcha_failed");

    client.captcha = Some("forged".to_owned());
    let reply = client
        .send(
            Method::POST,
            "/v1/auth/register",
            Some(client.with_captcha(body)),
        )
        .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);
    assert_eq!(code(&reply.body), "captcha_failed");

    client.captcha = Some(GOOD.to_owned());
    // Asserts the 202 and the verification email.
    client.register("Ada", &email, PASSWORD).await;
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn login_needs_a_valid_captcha(pool: PgPool) {
    let (url, seen) = siteverify().await;
    let mut client = guarded(pool, &url).await;
    let email = unique_email("captcha-login");
    client.captcha = Some(GOOD.to_owned());
    client.register_verified("Ada", &email, PASSWORD).await;

    client.captcha = None;
    let reply = client.login(&email, PASSWORD).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);
    assert_eq!(code(&reply.body), "captcha_failed");

    client.captcha = Some(GOOD.to_owned());
    let reply = client.login(&email, PASSWORD).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert!(client.token.is_some());

    // Cloudflare gets the secret and the visitor's address along with the token.
    let last = seen.lock().unwrap().last().cloned().unwrap();
    assert_eq!(last["secret"], SECRET);
    assert_eq!(last["response"], GOOD);
    assert_eq!(last["remoteip"], client.ip.to_string());
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn failed_captchas_do_not_lock_the_account(pool: PgPool) {
    let (url, _) = siteverify().await;
    let mut client = guarded(pool, &url).await;
    let email = unique_email("captcha-lock");
    client.captcha = Some(GOOD.to_owned());
    client.register_verified("Ada", &email, PASSWORD).await;

    // More failures than the five wrong passwords that lock an account.
    client.captcha = Some("forged".to_owned());
    for _ in 0..6 {
        let reply = client.login(&email, PASSWORD).await;
        assert_eq!(code(&reply.body), "captcha_failed");
    }
    client.captcha = Some(GOOD.to_owned());
    let reply = client.login(&email, PASSWORD).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn password_recovery_needs_a_valid_captcha(pool: PgPool) {
    let (url, _) = siteverify().await;
    let mut client = guarded(pool, &url).await;
    let email = unique_email("captcha-reset");

    let reply = client
        .send(
            Method::POST,
            "/v1/auth/forgot-password",
            Some(json!({ "email": email })),
        )
        .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);
    assert_eq!(code(&reply.body), "captcha_failed");

    let reply = client
        .send(
            Method::POST,
            "/v1/auth/forgot-password",
            Some(json!({ "email": email, "captcha_token": GOOD })),
        )
        .await;
    assert_eq!(reply.status, StatusCode::ACCEPTED, "{}", reply.body);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn an_unreachable_siteverify_answers_503(pool: PgPool) {
    // An address nothing listens on.
    let closed: SocketAddr = {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap()
    };
    let mut client = guarded(pool, &format!("http://{closed}/siteverify")).await;
    client.captcha = Some(GOOD.to_owned());

    let reply = client.login(&unique_email("captcha-down"), PASSWORD).await;
    assert_eq!(
        reply.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        reply.body
    );
}

#[test]
fn the_secret_is_required_outside_development() {
    let mut settings = Settings::default();
    for env in [Environment::Staging, Environment::Prod] {
        settings.app.env = env;
        assert!(taut_auth::captcha(&settings).is_err(), "{env:?}");
    }
    for env in [Environment::Dev, Environment::Test] {
        settings.app.env = env;
        assert!(
            !taut_auth::captcha(&settings).unwrap().is_enabled(),
            "{env:?}"
        );
    }
    settings.app.env = Environment::Prod;
    settings.auth.turnstile_secret = SECRET.to_owned();
    assert!(taut_auth::captcha(&settings).unwrap().is_enabled());
}
