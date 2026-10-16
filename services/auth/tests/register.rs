//! Registration, verification and the enumeration-safe answers (needs `just up`).

mod common;

use http::{Method, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use taut_model::events::AccountEmail;

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn register_verify_and_login(pool: PgPool) {
    let mut client = common::client(pool).await;
    let email = common::unique_email("ana");

    let token = client
        .register("  Ana  ", &email, "correct horse battery")
        .await;

    let before = client.login(&email, "correct horse battery").await;
    assert_eq!(before.status, StatusCode::FORBIDDEN);
    assert_eq!(before.body["error"]["code"], "email_unverified");

    let verified = client
        .send(
            Method::POST,
            "/v1/auth/verify-email",
            Some(json!({ "token": token })),
        )
        .await;
    assert_eq!(verified.status, StatusCode::NO_CONTENT);

    let again = client
        .send(
            Method::POST,
            "/v1/auth/verify-email",
            Some(json!({ "token": token })),
        )
        .await;
    assert_eq!(again.status, StatusCode::BAD_REQUEST, "a link works once");

    let login = client.login(&email, "correct horse battery").await;
    assert_eq!(login.status, StatusCode::OK, "{}", login.body);
    assert_eq!(login.body["token_type"], "Bearer");
    assert_eq!(login.body["expires_in"], 900);
    assert_eq!(login.body["user"]["name"], "Ana");
    assert_eq!(login.body["user"]["email"], email);
    assert!(login.body["user"]["email_verified_at"].is_string());
    assert!(client.cookie.is_some(), "refresh cookie set");
    let set_cookie = login.headers.get("set-cookie").unwrap().to_str().unwrap();
    assert!(set_cookie.contains("HttpOnly"), "{set_cookie}");
    assert!(set_cookie.contains("SameSite=Lax"), "{set_cookie}");
    assert!(set_cookie.contains("Path=/v1/auth"), "{set_cookie}");
    assert!(
        !set_cookie.contains("Secure"),
        "plain http in dev: {set_cookie}"
    );

    let me = client.send(Method::GET, "/v1/auth/me", None).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["email"], email);
    assert_eq!(me.body["mfa_enabled"], false);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn registering_an_existing_email_sends_a_notice_instead(pool: PgPool) {
    let mut client = common::client(pool).await;
    let email = common::unique_email("dup");
    client
        .register_verified("Ana", &email, "correct horse battery")
        .await;

    let mut emails = client.emails().await;
    let reply = client
        .send(
            Method::POST,
            "/v1/auth/register",
            Some(json!({ "name": "Impostor", "email": email, "password": "another password" })),
        )
        .await;
    assert_eq!(
        reply.status,
        StatusCode::ACCEPTED,
        "same answer as a new account"
    );
    let notice = common::next_email_to(&mut emails, &email).await;
    assert!(
        matches!(notice, AccountEmail::AlreadyRegistered { ref name, .. } if name == "Ana"),
        "{notice:?}"
    );

    let login = client.login(&email, "another password").await;
    assert_eq!(
        login.status,
        StatusCode::UNAUTHORIZED,
        "the password did not change"
    );
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn registering_again_unverified_resends_the_link(pool: PgPool) {
    let mut client = common::client(pool).await;
    let email = common::unique_email("twice");
    let first = client
        .register("Ana", &email, "correct horse battery")
        .await;
    let second = client
        .register("Ana", &email, "correct horse battery")
        .await;
    assert_ne!(first, second);

    let old = client
        .send(
            Method::POST,
            "/v1/auth/verify-email",
            Some(json!({ "token": first })),
        )
        .await;
    assert_eq!(
        old.status,
        StatusCode::BAD_REQUEST,
        "only the newest link works"
    );
    let new = client
        .send(
            Method::POST,
            "/v1/auth/verify-email",
            Some(json!({ "token": second })),
        )
        .await;
    assert_eq!(new.status, StatusCode::NO_CONTENT);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn invalid_input_is_a_bad_request(pool: PgPool) {
    let mut client = common::client(pool).await;
    for body in [
        json!({ "name": "Ana", "email": "not-an-email", "password": "correct horse battery" }),
        json!({ "name": "", "email": "a@b.co", "password": "correct horse battery" }),
        json!({ "name": "Ana", "email": "a@b.co", "password": "short" }),
        json!({ "name": "Ana", "email": "ana@b.co", "password": "ana@b.co" }),
        json!({ "name": "Ana", "email": "a@b.co" }),
        json!({ "name": "Ana", "email": "a@b.co", "password": "correct horse", "role": "owner" }),
    ] {
        let reply = client
            .send(Method::POST, "/v1/auth/register", Some(body.clone()))
            .await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(reply.body["error"]["code"], "bad_request", "{body}");
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn resend_verification_only_emails_unverified_accounts(pool: PgPool) {
    let mut client = common::client(pool).await;
    let email = common::unique_email("resend");
    client
        .register("Ana", &email, "correct horse battery")
        .await;

    let mut emails = client.emails().await;
    let reply = client
        .send(
            Method::POST,
            "/v1/auth/resend-verification",
            Some(json!({ "email": email })),
        )
        .await;
    assert_eq!(reply.status, StatusCode::ACCEPTED);
    let sent = common::next_email_to(&mut emails, &email).await;
    assert!(matches!(sent, AccountEmail::VerificationRequested { .. }));

    let unknown = client
        .send(
            Method::POST,
            "/v1/auth/resend-verification",
            Some(json!({ "email": common::unique_email("nobody") })),
        )
        .await;
    assert_eq!(
        unknown.status,
        StatusCode::ACCEPTED,
        "same answer for unknown addresses"
    );
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn jwks_publishes_the_signing_key(pool: PgPool) {
    let mut client = common::client(pool).await;
    let reply = client
        .send(Method::GET, "/.well-known/jwks.json", None)
        .await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["keys"][0]["kid"], client.keys.kid());
    assert_eq!(reply.body["keys"][0]["kty"], "OKP");
    assert_eq!(
        reply.headers.get("cache-control").unwrap(),
        "public, max-age=300"
    );
}
