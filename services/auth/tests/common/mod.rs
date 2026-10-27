//! Helpers shared by the auth integration tests. They need the Redis and NATS of `just up`
//! besides the PostgreSQL that `#[sqlx::test]` provides.

#![allow(dead_code)] // each test binary uses a different subset

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use futures::StreamExt;
use http::header::{AUTHORIZATION, CONTENT_TYPE, COOKIE, SET_COOKIE};
use http::{HeaderMap, Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use taut_bus::Bus;
use taut_config::Settings;
use taut_identity::KeyPair;
use taut_model::events::AccountEmail;
use tower::ServiceExt;
use uuid::Uuid;

/// The auth router plus everything a test needs to talk to it as one browser: its own
/// client address (so the per-address limits never mix tests), the refresh cookie, the
/// access token of the last login and the captcha token its forms send.
pub struct Client {
    pub router: Router,
    pub keys: KeyPair,
    pub bus: Bus,
    pub ip: IpAddr,
    pub cookie: Option<String>,
    pub token: Option<String>,
    /// `captcha_token` of `register` and `login`; `None` sends none.
    pub captcha: Option<String>,
}

/// A response: status, JSON body (`Null` when empty) and headers.
pub struct Reply {
    pub status: StatusCode,
    pub body: Value,
    pub headers: HeaderMap,
}

/// Builds the service over `pool` with a fresh key pair and the settings of `.env`, with the
/// captcha off whatever `.env` says.
pub async fn client(pool: PgPool) -> Client {
    client_with(pool, |settings| settings.auth.turnstile_secret.clear()).await
}

/// Like [`client`], with `adjust` applied to the settings of `.env` first.
pub async fn client_with(pool: PgPool, adjust: impl FnOnce(&mut Settings)) -> Client {
    let mut settings = Settings::load().expect("settings load");
    adjust(&mut settings);
    let bus = Bus::connect(&settings.nats, "auth-tests")
        .await
        .expect("nats is up (just up)");
    let redis = redis::Client::open(settings.redis.url.as_str())
        .expect("redis url")
        .get_connection_manager()
        .await
        .expect("redis is up (just up)");
    let telemetry = taut_telemetry::init(taut_auth::SERVICE_NAME, &settings.telemetry)
        .expect("telemetry installs once per process");
    let keys = KeyPair::generate();
    let totp = taut_auth::totp_cipher(&settings).expect("dev totp key");
    let captcha = taut_auth::captcha(&settings).expect("captcha settings");
    let router = taut_auth::app(
        Arc::new(settings),
        pool,
        bus.clone(),
        redis,
        keys.clone(),
        totp,
        captcha,
        telemetry.metrics().clone(),
    );
    // nextest runs every test in its own process, so a counter would repeat: random it is.
    let [a, b, c, ..] = taut_identity::random::token_bytes();
    let ip = IpAddr::V4(Ipv4Addr::new(10, a, b, c));
    Client {
        router,
        keys,
        bus,
        ip,
        cookie: None,
        token: None,
        captcha: None,
    }
}

/// A unique address per test, so Redis counters and rows never collide across tests.
pub fn unique_email(prefix: &str) -> String {
    format!("{prefix}-{}@test.taut", Uuid::now_v7().simple())
}

impl Client {
    /// Sends a request from this client's address, with its cookie and bearer token when set.
    /// Updates the cookie from `Set-Cookie`.
    pub async fn send(&mut self, method: Method, path: &str, body: Option<Value>) -> Reply {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(cookie) = &self.cookie {
            request = request.header(COOKIE, format!("taut_refresh={cookie}"));
        }
        if let Some(token) = &self.token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        let mut request = match body {
            Some(body) => request
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .expect("valid request");
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::new(self.ip, 40000)));
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("router answers");
        let status = response.status();
        let headers = response.headers().clone();
        for value in headers.get_all(SET_COOKIE) {
            let value = value.to_str().unwrap();
            if let Some(rest) = value.strip_prefix("taut_refresh=") {
                let token = rest.split(';').next().unwrap_or("");
                self.cookie = (!token.is_empty()).then(|| token.to_owned());
            }
        }
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
        Reply {
            status,
            body,
            headers,
        }
    }

    /// `body` plus this client's `captcha_token`, when it has one.
    pub fn with_captcha(&self, mut body: Value) -> Value {
        if let Some(captcha) = &self.captcha {
            body["captcha_token"] = Value::String(captcha.clone());
        }
        body
    }

    /// Subscribes to the account emails before an action, so none is missed.
    pub async fn emails(&self) -> async_nats::Subscriber {
        self.bus
            .client()
            .subscribe("users.>")
            .await
            .expect("subscribe")
    }

    /// Registers `email`, waits for its verification email and returns the token in the link.
    pub async fn register(&mut self, name: &str, email: &str, password: &str) -> String {
        let mut emails = self.emails().await;
        let reply = self
            .send(
                Method::POST,
                "/v1/auth/register",
                Some(
                    self.with_captcha(
                        json!({ "name": name, "email": email, "password": password }),
                    ),
                ),
            )
            .await;
        assert_eq!(reply.status, StatusCode::ACCEPTED, "{}", reply.body);
        let email = next_email_to(&mut emails, email).await;
        let AccountEmail::VerificationRequested { link, .. } = email else {
            panic!("expected a verification email, got {email:?}");
        };
        token_of(&link)
    }

    /// Registers and verifies `email`.
    pub async fn register_verified(&mut self, name: &str, email: &str, password: &str) {
        let token = self.register(name, email, password).await;
        let reply = self
            .send(
                Method::POST,
                "/v1/auth/verify-email",
                Some(json!({ "token": token })),
            )
            .await;
        assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    }

    /// Logs in and keeps the access token and the refresh cookie.
    pub async fn login(&mut self, email: &str, password: &str) -> Reply {
        let reply = self
            .send(
                Method::POST,
                "/v1/auth/login",
                Some(self.with_captcha(json!({ "email": email, "password": password }))),
            )
            .await;
        if reply.status == StatusCode::OK {
            self.token = reply.body["access_token"].as_str().map(ToOwned::to_owned);
        }
        reply
    }
}

/// The next account email addressed to `to`.
pub async fn next_email_to(subscriber: &mut async_nats::Subscriber, to: &str) -> AccountEmail {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = subscriber.next().await {
            let email: AccountEmail = serde_json::from_slice(&message.payload).unwrap();
            if email.to() == to {
                return email;
            }
        }
        panic!("subscription closed");
    })
    .await
    .expect("email published")
}

/// The `token` query parameter of a link.
pub fn token_of(link: &str) -> String {
    link.split("token=")
        .nth(1)
        .expect("link carries a token")
        .to_owned()
}
