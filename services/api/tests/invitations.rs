//! Invitations: owners invite by email, the invited account accepts (needs `just up`).

mod common;

use std::time::Duration;

use futures::StreamExt;
use http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use taut_model::events::{AccountEmail, subjects};
use uuid::Uuid;

/// An address no other test uses, so parallel tests on the shared NATS do not mix emails.
fn unique_email(prefix: &str) -> String {
    format!("{prefix}-{}@test.taut", Uuid::now_v7().simple())
}

async fn invitation_emails() -> async_nats::Subscriber {
    common::bus()
        .await
        .client()
        .subscribe(subjects::TENANTS_MEMBER_INVITED)
        .await
        .expect("subscribe")
}

/// The secret of the link of the next invitation sent to `to`.
async fn next_token_for(subscriber: &mut async_nats::Subscriber, to: &str) -> String {
    let link = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = subscriber.next().await {
            let email: AccountEmail = serde_json::from_slice(&message.payload).unwrap();
            if let AccountEmail::MemberInvited {
                to: sent_to, link, ..
            } = email
                && sent_to == to
            {
                return link;
            }
        }
        panic!("subscription closed");
    })
    .await
    .expect("invitation email published");
    let (base, token) = link
        .rsplit_once("/invite/")
        .expect("link to /invite/{token}");
    assert_eq!(base, "http://localhost:5173");
    token.to_owned()
}

/// Invites `email` to `tenant` as `role` and returns the response body and the link's secret.
async fn invite(app: &common::TestApp, tenant: &str, email: &str, role: &str) -> (Value, String) {
    let mut emails = invitation_emails().await;
    let (status, body) = common::send(
        app,
        Method::POST,
        "/v1/invitations",
        Some(tenant),
        Some(json!({ "email": email, "role": role })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let token = next_token_for(&mut emails, &email.to_lowercase()).await;
    (body, token)
}

/// Status of `GET /v1/invitations/{token}`, without a session.
async fn preview(app: &common::TestApp, token: &str) -> StatusCode {
    let path = format!("/v1/invitations/{token}");
    common::send_raw(app, Method::GET, &path, None, None, None)
        .await
        .0
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn the_invited_account_accepts_and_joins(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let email = unique_email("bea");
    let (invitation, token) = invite(&app, &tenant, &email.to_uppercase(), "member").await;
    assert_eq!(invitation["email"], email.as_str());
    assert_eq!(invitation["role"], "member");
    assert_eq!(invitation["invited_by"], "owner");

    let (status, pending) =
        common::send(&app, Method::GET, "/v1/invitations", Some(&tenant), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pending.as_array().unwrap().len(), 1, "{pending}");

    // The link alone shows the invitation; no session needed.
    let (status, preview) = common::send_raw(
        &app,
        Method::GET,
        &format!("/v1/invitations/{token}"),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["tenant_name"], "Tenant acme");
    assert_eq!(preview["email"], email.as_str());
    assert_eq!(preview["role"], "member");

    // Accepting needs a session.
    let accept = format!("/v1/invitations/{token}/accept");
    let (status, _) = common::send_raw(&app, Method::POST, &accept, None, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let bea = app.as_user(common::insert_user(&app.pool, &email).await);
    let (status, joined) = common::send(&bea, Method::POST, &accept, None, None).await;
    assert_eq!(status, StatusCode::OK, "{joined}");
    assert_eq!(joined["id"], tenant.as_str());
    assert_eq!(joined["role"], "member");

    let (status, members) =
        common::send(&bea, Method::GET, "/v1/members", Some(&tenant), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(members.as_array().unwrap().len(), 2, "{members}");

    // Used links are gone, and accepted invitations leave the pending list.
    let (status, _) = common::send(&bea, Method::POST, &accept, None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, pending) =
        common::send(&app, Method::GET, "/v1/invitations", Some(&tenant), None).await;
    assert_eq!(pending, json!([]));
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn another_account_cannot_accept(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let (_, token) = invite(&app, &tenant, &unique_email("bea"), "owner").await;

    let eve = app.as_user(common::insert_user(&app.pool, &unique_email("eve")).await);
    let (status, body) = common::send(
        &eve,
        Method::POST,
        &format!("/v1/invitations/{token}/accept"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "forbidden");
    let (status, _) = common::send(&eve, Method::GET, "/v1/members", Some(&tenant), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn expired_invitations_are_rejected(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let email = unique_email("bea");
    let (invitation, token) = invite(&app, &tenant, &email, "member").await;
    sqlx::query(
        "UPDATE invitations SET expires_at = now() - interval '1 minute' WHERE id = $1::uuid",
    )
    .bind(invitation["id"].as_str().unwrap())
    .execute(&app.pool)
    .await
    .unwrap();

    let (status, body) = common::send_raw(
        &app,
        Method::GET,
        &format!("/v1/invitations/{token}"),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let bea = app.as_user(common::insert_user(&app.pool, &email).await);
    let (status, body) = common::send(
        &bea,
        Method::POST,
        &format!("/v1/invitations/{token}/accept"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["message"], "the invitation expired");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn owners_revoke_and_reinvite(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let email = unique_email("bea");
    let (first, first_token) = invite(&app, &tenant, &email, "member").await;

    // Inviting again replaces the pending invitation: only the newest link works.
    let (second, second_token) = invite(&app, &tenant, &email, "owner").await;
    assert_ne!(first["id"], second["id"]);
    assert_eq!(preview(&app, &first_token).await, StatusCode::NOT_FOUND);
    assert_eq!(preview(&app, &second_token).await, StatusCode::OK);

    let path = format!("/v1/invitations/{}", second["id"].as_str().unwrap());
    let (status, _) = common::send(&app, Method::DELETE, &path, Some(&tenant), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(preview(&app, &second_token).await, StatusCode::NOT_FOUND);
    let (status, _) = common::send(&app, Method::DELETE, &path, Some(&tenant), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn only_owners_invite_and_members_are_not_invited_twice(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let member_email = unique_email("max");
    let max = common::insert_user(&app.pool, &member_email).await;
    common::add_member(&app.pool, &tenant, max, "member").await;
    let max = app.as_user(max);

    for (method, body) in [
        (Method::GET, None),
        (
            Method::POST,
            Some(json!({ "email": unique_email("x"), "role": "member" })),
        ),
    ] {
        let (status, body) =
            common::send(&max, method, "/v1/invitations", Some(&tenant), body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    }

    for (body, expected) in [
        (
            json!({ "email": member_email, "role": "member" }),
            StatusCode::CONFLICT,
        ),
        (
            json!({ "email": "not-an-email", "role": "member" }),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({ "email": unique_email("x"), "role": "admin" }),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let (status, response) = common::send(
            &app,
            Method::POST,
            "/v1/invitations",
            Some(&tenant),
            Some(body.clone()),
        )
        .await;
        assert_eq!(status, expected, "{body}: {response}");
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn accepting_twice_into_the_same_tenant_conflicts(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let email = unique_email("bea");
    let (_, token) = invite(&app, &tenant, &email, "member").await;
    // Joined some other way (an older invitation) before using this link.
    let bea = common::insert_user(&app.pool, &email).await;
    common::add_member(&app.pool, &tenant, bea, "member").await;

    let (status, body) = common::send(
        &app.as_user(bea),
        Method::POST,
        &format!("/v1/invitations/{token}/accept"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn a_tenant_has_at_most_twenty_pending_invitations(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    for n in 0..20 {
        let (status, body) = common::send(
            &app,
            Method::POST,
            "/v1/invitations",
            Some(&tenant),
            Some(json!({ "email": unique_email(&format!("u{n}")), "role": "member" })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    let (status, body) = common::send(
        &app,
        Method::POST,
        "/v1/invitations",
        Some(&tenant),
        Some(json!({ "email": unique_email("one-too-many"), "role": "member" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}
