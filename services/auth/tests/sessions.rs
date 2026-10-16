//! Login limits, refresh rotation, logout and password changes (needs `just up`).

mod common;

use http::{Method, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use taut_identity::Verifier;
use taut_model::events::AccountEmail;

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn wrong_passwords_lock_the_account_after_five_tries(pool: PgPool) {
    let mut client = common::client(pool).await;
    let email = common::unique_email("lock");
    client
        .register_verified("Ana", &email, "correct horse battery")
        .await;

    for _ in 0..5 {
        let reply = client.login(&email, "wrong").await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
        assert_eq!(reply.body["error"]["code"], "unauthorized");
    }
    let locked = client.login(&email, "correct horse battery").await;
    assert_eq!(
        locked.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{}",
        locked.body
    );
    assert_eq!(locked.body["error"]["code"], "too_many_requests");
    let retry_after: u64 = locked
        .headers
        .get("retry-after")
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=900).contains(&retry_after), "{retry_after}");

    // Unknown accounts answer the same as a wrong password.
    let unknown = client
        .login(&common::unique_email("ghost"), "whatever")
        .await;
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn too_many_failures_from_one_address_are_limited(pool: PgPool) {
    let mut client = common::client(pool).await;
    for i in 0..20 {
        let reply = client
            .login(&format!("ghost-{i}-{}@test.taut", client.ip), "whatever")
            .await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "attempt {i}");
    }
    let reply = client.login("one-more@test.taut", "whatever").await;
    assert_eq!(reply.status, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn refresh_rotates_and_a_replayed_cookie_kills_the_session(pool: PgPool) {
    let mut client = common::client(pool).await;
    let email = common::unique_email("rotate");
    client
        .register_verified("Ana", &email, "correct horse battery")
        .await;
    client.login(&email, "correct horse battery").await;
    let first_cookie = client.cookie.clone().unwrap();
    let first_token = client.token.clone().unwrap();

    let refreshed = client.send(Method::POST, "/v1/auth/refresh", None).await;
    assert_eq!(refreshed.status, StatusCode::OK, "{}", refreshed.body);
    let second_cookie = client.cookie.clone().unwrap();
    assert_ne!(first_cookie, second_cookie, "the cookie rotates");
    let second_token = refreshed.body["access_token"].as_str().unwrap().to_owned();
    assert_ne!(first_token, second_token);

    // Both tokens name the same session.
    let verifier = Verifier::from_jwks(&client.keys.jwks(), "taut-auth");
    let first = verifier.verify_access(&first_token).await.unwrap();
    let second = verifier.verify_access(&second_token).await.unwrap();
    assert_eq!(first.sid, second.sid);
    assert_eq!(
        first.auth_time, second.auth_time,
        "a refresh is not a login"
    );

    // Replaying the first cookie is an attack: the whole session dies.
    client.cookie = Some(first_cookie);
    let replay = client.send(Method::POST, "/v1/auth/refresh", None).await;
    assert_eq!(replay.status, StatusCode::UNAUTHORIZED);
    assert!(client.cookie.is_none(), "the cookie is cleared");

    client.cookie = Some(second_cookie);
    let dead = client.send(Method::POST, "/v1/auth/refresh", None).await;
    assert_eq!(
        dead.status,
        StatusCode::UNAUTHORIZED,
        "the current cookie is dead too"
    );
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn refresh_without_a_cookie_and_logout_are_handled(pool: PgPool) {
    let mut client = common::client(pool).await;
    let reply = client.send(Method::POST, "/v1/auth/refresh", None).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);

    let email = common::unique_email("logout");
    client
        .register_verified("Ana", &email, "correct horse battery")
        .await;
    client.login(&email, "correct horse battery").await;
    let cookie = client.cookie.clone().unwrap();

    let out = client.send(Method::POST, "/v1/auth/logout", None).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert!(client.cookie.is_none());

    client.cookie = Some(cookie);
    let after = client.send(Method::POST, "/v1/auth/refresh", None).await;
    assert_eq!(
        after.status,
        StatusCode::UNAUTHORIZED,
        "the session is gone"
    );

    let again = client.send(Method::POST, "/v1/auth/logout", None).await;
    assert_eq!(again.status, StatusCode::NO_CONTENT, "logout is idempotent");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn me_requires_a_valid_access_token(pool: PgPool) {
    let mut client = common::client(pool).await;
    let reply = client.send(Method::GET, "/v1/auth/me", None).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);

    client.token = Some("taut_live_notatoken".to_owned());
    let reply = client.send(Method::GET, "/v1/auth/me", None).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);

    client.token = Some("eyJ.garbage.sig".to_owned());
    let reply = client.send(Method::GET, "/v1/auth/me", None).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn changing_the_password_ends_the_other_sessions(pool: PgPool) {
    let mut client = common::client(pool).await;
    let email = common::unique_email("change");
    client
        .register_verified("Ana", &email, "correct horse battery")
        .await;
    client.login(&email, "correct horse battery").await;
    let other_cookie = client.cookie.clone().unwrap();
    let other_token = client.token.clone().unwrap();

    // A second browser.
    client.login(&email, "correct horse battery").await;

    let wrong = client
        .send(
            Method::PUT,
            "/v1/auth/password",
            Some(json!({ "current_password": "nope", "new_password": "new horse battery" })),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);

    let changed = client
        .send(
            Method::PUT,
            "/v1/auth/password",
            Some(json!({
                "current_password": "correct horse battery",
                "new_password": "new horse battery"
            })),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    client.token = changed.body["access_token"].as_str().map(ToOwned::to_owned);
    let me = client.send(Method::GET, "/v1/auth/me", None).await;
    assert_eq!(me.status, StatusCode::OK, "this browser stays signed in");

    // The first browser is out: its refresh fails and its access token no longer works here.
    client.cookie = Some(other_cookie);
    let stale = client.send(Method::POST, "/v1/auth/refresh", None).await;
    assert_eq!(stale.status, StatusCode::UNAUTHORIZED);
    client.token = Some(other_token);
    let stale_me = client.send(Method::GET, "/v1/auth/me", None).await;
    assert_eq!(stale_me.status, StatusCode::UNAUTHORIZED);

    let login = client.login(&email, "new horse battery").await;
    assert_eq!(login.status, StatusCode::OK);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn forgotten_passwords_are_reset_by_email(pool: PgPool) {
    let mut client = common::client(pool).await;
    let email = common::unique_email("reset");
    client
        .register_verified("Ana", &email, "correct horse battery")
        .await;
    client.login(&email, "correct horse battery").await;

    let mut emails = client.emails().await;
    let asked = client
        .send(
            Method::POST,
            "/v1/auth/forgot-password",
            Some(json!({ "email": email })),
        )
        .await;
    assert_eq!(asked.status, StatusCode::ACCEPTED);
    let sent = common::next_email_to(&mut emails, &email).await;
    let AccountEmail::PasswordResetRequested { link, .. } = sent else {
        panic!("expected a reset email, got {sent:?}");
    };
    let token = common::token_of(&link);

    let weak = client
        .send(
            Method::POST,
            "/v1/auth/reset-password",
            Some(json!({ "token": token, "password": "short" })),
        )
        .await;
    assert_eq!(weak.status, StatusCode::BAD_REQUEST);

    let reset = client
        .send(
            Method::POST,
            "/v1/auth/reset-password",
            Some(json!({ "token": token, "password": "reset horse battery" })),
        )
        .await;
    assert_eq!(reset.status, StatusCode::NO_CONTENT, "{}", reset.body);

    let stale = client.send(Method::POST, "/v1/auth/refresh", None).await;
    assert_eq!(
        stale.status,
        StatusCode::UNAUTHORIZED,
        "old sessions are gone"
    );

    let reused = client
        .send(
            Method::POST,
            "/v1/auth/reset-password",
            Some(json!({ "token": token, "password": "reset horse battery" })),
        )
        .await;
    assert_eq!(
        reused.status,
        StatusCode::BAD_REQUEST,
        "a reset link works once"
    );

    let login = client.login(&email, "reset horse battery").await;
    assert_eq!(login.status, StatusCode::OK);

    let unknown = client
        .send(
            Method::POST,
            "/v1/auth/forgot-password",
            Some(json!({ "email": common::unique_email("nobody") })),
        )
        .await;
    assert_eq!(
        unknown.status,
        StatusCode::ACCEPTED,
        "same answer for unknown addresses"
    );
}
