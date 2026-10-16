//! Second factor: enrollment, two-step login, replay, backup codes, re-authentication and
//! step-up (needs `just up`).

mod common;

use std::collections::HashSet;

use chrono::Utc;
use http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use taut_identity::{Claims, TokenKind, Verifier};
use totp_rs::{Builder, Secret, Totp};
use uuid::Uuid;

const PASSWORD: &str = "correct horse battery";

fn now() -> u64 {
    u64::try_from(Utc::now().timestamp()).unwrap()
}

/// The authenticator app, rebuilt from the `secret` that `enroll` returned. The service
/// accepts each 30 s step once, so it remembers which steps it already handed out and picks
/// a fresh one within the ±1 window the service tolerates.
struct Authenticator {
    totp: Totp,
    used: HashSet<u64>,
}

impl Authenticator {
    fn from(enrollment: &Value) -> Self {
        let secret = Secret::try_from_base32(enrollment["secret"].as_str().unwrap()).unwrap();
        let uri = enrollment["otpauth_uri"].as_str().unwrap();
        assert!(uri.starts_with("otpauth://totp/taut:"), "{enrollment}");
        assert!(uri.contains("issuer=taut"), "{enrollment}");
        let totp = Builder::new()
            .with_secret(secret.as_bytes().to_vec())
            .build()
            .unwrap();
        Self {
            totp,
            used: HashSet::new(),
        }
    }

    /// A code the service has not seen yet.
    fn next_code(&mut self) -> String {
        let step = now() / 30;
        let fresh = [step, step + 1, step - 1]
            .into_iter()
            .find(|candidate| !self.used.contains(candidate))
            .expect("a test never needs more than three codes per window");
        self.used.insert(fresh);
        self.totp.generate(fresh * 30).to_string()
    }

    /// A code that is certainly wrong: the current one plus one.
    fn wrong_code(&self) -> String {
        let current: u32 = self.totp.generate(now()).to_string().parse().unwrap();
        format!("{:06}", (current + 1) % 1_000_000)
    }
}

/// Registers, logs in and turns 2FA on; returns the app and the backup codes.
async fn enable_mfa(client: &mut common::Client, email: &str) -> (Authenticator, Vec<String>) {
    client.register_verified("Ana", email, PASSWORD).await;
    let login = client.login(email, PASSWORD).await;
    assert_eq!(login.status, StatusCode::OK, "{}", login.body);
    assert_eq!(login.body["user"]["mfa_enabled"], false);

    let enrolled = client.send(Method::POST, "/v1/auth/mfa/enroll", None).await;
    assert_eq!(enrolled.status, StatusCode::OK, "{}", enrolled.body);
    let mut app = Authenticator::from(&enrolled.body);

    let wrong = client
        .send(
            Method::POST,
            "/v1/auth/mfa/confirm",
            Some(json!({ "code": app.wrong_code() })),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED, "{}", wrong.body);
    let me = client.send(Method::GET, "/v1/auth/me", None).await;
    assert_eq!(me.body["mfa_enabled"], false, "not enabled by a wrong code");

    let confirmed = client
        .send(
            Method::POST,
            "/v1/auth/mfa/confirm",
            Some(json!({ "code": app.next_code() })),
        )
        .await;
    assert_eq!(confirmed.status, StatusCode::OK, "{}", confirmed.body);
    let codes: Vec<String> =
        serde_json::from_value(confirmed.body["backup_codes"].clone()).unwrap();
    assert_eq!(codes.len(), 8);
    for code in &codes {
        assert_eq!(code.len(), 9, "{code}");
        assert_eq!(&code[4..5], "-");
    }
    (app, codes)
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn enrolling_turns_the_second_factor_on_and_the_login_takes_two_steps(pool: PgPool) {
    let mut client = common::client(pool).await;
    let email = common::unique_email("mfa");
    let (mut app, _codes) = enable_mfa(&mut client, &email).await;

    let me = client.send(Method::GET, "/v1/auth/me", None).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["mfa_enabled"], true);
    let status = client.send(Method::GET, "/v1/auth/mfa", None).await;
    assert_eq!(status.body["enabled"], true);
    assert_eq!(status.body["backup_codes_left"], 8);

    let again = client.send(Method::POST, "/v1/auth/mfa/enroll", None).await;
    assert_eq!(again.status, StatusCode::CONFLICT, "already enabled");

    // A new browser: the password alone no longer opens a session.
    client.cookie = None;
    client.token = None;
    let first = client.login(&email, PASSWORD).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    assert_eq!(first.body["status"], "mfa_required");
    assert!(first.body["access_token"].is_null());
    assert!(client.cookie.is_none(), "no session cookie yet");
    let mfa_token = first.body["mfa_token"].as_str().unwrap().to_owned();

    // The MFA token is not an access token.
    client.token = Some(mfa_token.clone());
    let me = client.send(Method::GET, "/v1/auth/me", None).await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
    client.token = None;

    let wrong = client
        .send(
            Method::POST,
            "/v1/auth/mfa",
            Some(json!({ "mfa_token": mfa_token, "code": "12345" })),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST, "not six digits");

    let second = client
        .send(
            Method::POST,
            "/v1/auth/mfa",
            Some(json!({ "mfa_token": mfa_token, "code": app.next_code() })),
        )
        .await;
    assert_eq!(second.status, StatusCode::OK, "{}", second.body);
    assert!(client.cookie.is_some(), "the session cookie arrives now");
    let access = second.body["access_token"].as_str().unwrap().to_owned();
    let verifier = Verifier::from_jwks(&client.keys.jwks(), "taut-auth");
    let claims = verifier.verify_access(&access).await.unwrap();
    assert!(claims.mfa, "the session passed the second factor");

    // The same MFA token cannot complete a second session.
    let reused = client
        .send(
            Method::POST,
            "/v1/auth/mfa",
            Some(json!({ "mfa_token": mfa_token, "code": app.next_code() })),
        )
        .await;
    assert_eq!(reused.status, StatusCode::UNAUTHORIZED, "{}", reused.body);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn a_totp_code_works_once(pool: PgPool) {
    let mut client = common::client(pool).await;
    let email = common::unique_email("replay");
    let (mut app, _codes) = enable_mfa(&mut client, &email).await;
    client.cookie = None;
    client.token = None;

    let code = app.next_code();
    let first = client.login(&email, PASSWORD).await;
    let ok = client
        .send(
            Method::POST,
            "/v1/auth/mfa",
            Some(json!({ "mfa_token": first.body["mfa_token"], "code": code })),
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.body);

    let again = client.login(&email, PASSWORD).await;
    let replay = client
        .send(
            Method::POST,
            "/v1/auth/mfa",
            Some(json!({ "mfa_token": again.body["mfa_token"], "code": code })),
        )
        .await;
    assert_eq!(replay.status, StatusCode::UNAUTHORIZED, "{}", replay.body);
    assert_eq!(replay.body["error"]["code"], "unauthorized");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn backup_codes_work_once_and_wrong_codes_are_limited(pool: PgPool) {
    let mut client = common::client(pool).await;
    let email = common::unique_email("backup");
    let (_app, codes) = enable_mfa(&mut client, &email).await;
    client.cookie = None;
    client.token = None;

    let login = client.login(&email, PASSWORD).await;
    let mfa_token = login.body["mfa_token"].as_str().unwrap().to_owned();
    let typed = codes[0].to_uppercase().replace('-', " ");
    let ok = client
        .send(
            Method::POST,
            "/v1/auth/mfa",
            Some(json!({ "mfa_token": mfa_token, "backup_code": typed })),
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.body);
    client.token = ok.body["access_token"].as_str().map(ToOwned::to_owned);
    let status = client.send(Method::GET, "/v1/auth/mfa", None).await;
    assert_eq!(status.body["backup_codes_left"], 7, "{}", status.body);

    client.cookie = None;
    client.token = None;
    let login = client.login(&email, PASSWORD).await;
    let mfa_token = login.body["mfa_token"].as_str().unwrap().to_owned();
    let spent = client
        .send(
            Method::POST,
            "/v1/auth/mfa",
            Some(json!({ "mfa_token": mfa_token, "backup_code": codes[0] })),
        )
        .await;
    assert_eq!(
        spent.status,
        StatusCode::UNAUTHORIZED,
        "a backup code works once"
    );

    for _ in 0..4 {
        let wrong = client
            .send(
                Method::POST,
                "/v1/auth/mfa",
                Some(json!({ "mfa_token": mfa_token, "backup_code": "zzzz-zzzz" })),
            )
            .await;
        assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    }
    let limited = client
        .send(
            Method::POST,
            "/v1/auth/mfa",
            Some(json!({ "mfa_token": mfa_token, "backup_code": codes[1] })),
        )
        .await;
    assert_eq!(
        limited.status,
        StatusCode::TOO_MANY_REQUESTS,
        "five wrong codes lock the second factor: {}",
        limited.body
    );
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn sensitive_routes_need_a_recent_authentication(pool: PgPool) {
    let mut client = common::client(pool).await;
    let email = common::unique_email("stepup");
    let (mut app, _codes) = enable_mfa(&mut client, &email).await;
    let fresh_token = client.token.clone().unwrap();
    let verifier = Verifier::from_jwks(&client.keys.jwks(), "taut-auth");
    let fresh = verifier.verify_access(&fresh_token).await.unwrap();

    // The same session, as the access token looks after 10 minutes of refreshes.
    let stale = client
        .keys
        .sign(&Claims {
            jti: "01JSTALE".to_owned(),
            iat: Utc::now().timestamp(),
            exp: Utc::now().timestamp() + 900,
            auth_time: Utc::now().timestamp() - 600,
            ..fresh.clone()
        })
        .unwrap();
    client.token = Some(stale);

    let password = json!({ "current_password": PASSWORD, "new_password": "new horse battery" });
    let denied = client
        .send(Method::PUT, "/v1/auth/password", Some(password.clone()))
        .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN, "{}", denied.body);
    assert_eq!(denied.body["error"]["code"], "reauth_required");
    let denied = client
        .send(Method::POST, "/v1/auth/mfa/backup-codes", None)
        .await;
    assert_eq!(denied.body["error"]["code"], "reauth_required");
    let denied = client
        .send(
            Method::POST,
            "/v1/auth/mfa/disable",
            Some(json!({ "password": PASSWORD, "code": "123456" })),
        )
        .await;
    assert_eq!(denied.body["error"]["code"], "reauth_required");

    // Re-authenticating with 2FA on needs the code too.
    let no_code = client
        .send(
            Method::POST,
            "/v1/auth/reauth",
            Some(json!({ "password": PASSWORD })),
        )
        .await;
    assert_eq!(no_code.status, StatusCode::BAD_REQUEST, "{}", no_code.body);
    let wrong_password = client
        .send(
            Method::POST,
            "/v1/auth/reauth",
            Some(json!({ "password": "nope", "code": app.wrong_code() })),
        )
        .await;
    assert_eq!(wrong_password.status, StatusCode::UNAUTHORIZED);

    let reauth = client
        .send(
            Method::POST,
            "/v1/auth/reauth",
            Some(json!({ "password": PASSWORD, "code": app.next_code() })),
        )
        .await;
    assert_eq!(reauth.status, StatusCode::OK, "{}", reauth.body);
    let renewed = verifier
        .verify_access(reauth.body["access_token"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(renewed.sid, fresh.sid, "same session");
    assert!(renewed.auth_time >= Utc::now().timestamp() - 5);
    client.token = reauth.body["access_token"].as_str().map(ToOwned::to_owned);

    // A refresh keeps the new auth_time: the cookie was not touched.
    let refreshed = client.send(Method::POST, "/v1/auth/refresh", None).await;
    assert_eq!(refreshed.status, StatusCode::OK, "{}", refreshed.body);
    let after = verifier
        .verify_access(refreshed.body["access_token"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(after.auth_time, renewed.auth_time);
    assert!(after.mfa);

    // Now the sensitive routes pass.
    let regenerated = client
        .send(Method::POST, "/v1/auth/mfa/backup-codes", None)
        .await;
    assert_eq!(regenerated.status, StatusCode::OK, "{}", regenerated.body);
    assert_eq!(
        regenerated.body["backup_codes"].as_array().unwrap().len(),
        8
    );
    let backup: Vec<String> =
        serde_json::from_value(regenerated.body["backup_codes"].clone()).unwrap();
    let disabled = client
        .send(
            Method::POST,
            "/v1/auth/mfa/disable",
            Some(json!({ "password": PASSWORD, "backup_code": backup[0] })),
        )
        .await;
    assert_eq!(disabled.status, StatusCode::NO_CONTENT, "{}", disabled.body);
    let me = client.send(Method::GET, "/v1/auth/me", None).await;
    assert_eq!(me.body["mfa_enabled"], false);

    // Without 2FA the login is one step again.
    client.cookie = None;
    client.token = None;
    let login = client.login(&email, PASSWORD).await;
    assert_eq!(login.status, StatusCode::OK);
    assert!(login.body["access_token"].is_string(), "{}", login.body);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn mfa_tokens_are_not_accepted_as_access_tokens_nor_the_reverse(pool: PgPool) {
    let mut client = common::client(pool).await;
    let email = common::unique_email("kinds");
    client.register_verified("Ana", &email, PASSWORD).await;
    client.login(&email, PASSWORD).await;
    let access = client.token.clone().unwrap();

    // An access token where an MFA token is expected.
    let reply = client
        .send(
            Method::POST,
            "/v1/auth/mfa",
            Some(json!({ "mfa_token": access, "code": "123456" })),
        )
        .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{}", reply.body);

    // A forged MFA token for a user without 2FA.
    let forged = client
        .keys
        .sign(&Claims {
            iss: "taut-auth".to_owned(),
            sub: Uuid::now_v7(),
            jti: "01JFORGED".to_owned(),
            sid: Uuid::nil(),
            iat: Utc::now().timestamp(),
            exp: Utc::now().timestamp() + 300,
            auth_time: Utc::now().timestamp(),
            mfa: false,
            ver: 1,
            kind: TokenKind::Mfa,
        })
        .unwrap();
    let reply = client
        .send(
            Method::POST,
            "/v1/auth/mfa",
            Some(json!({ "mfa_token": forged, "code": "123456" })),
        )
        .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{}", reply.body);
}
