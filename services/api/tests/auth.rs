//! Who may do what: bearer tokens, memberships and roles (needs `just up`).

mod common;

use http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use taut_identity::KeyPair;

fn http_monitor(name: &str) -> Value {
    json!({ "name": name, "type": "http", "target": "https://example.com/health" })
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn requests_without_a_valid_token_are_unauthorized(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let foreign = KeyPair::generate();
    let expired = common::token_with(&app.keys, app.user_id, -120);
    let other_key = common::token_for(&foreign, app.user_id);
    for token in [
        None,
        Some("garbage"),
        Some("taut_live_notyet"),
        Some(expired.as_str()),
        Some(other_key.as_str()),
    ] {
        for path in ["/v1/tenants", "/v1/monitors"] {
            let (status, body) =
                common::send_raw(&app, Method::GET, path, token, Some(&tenant), None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{token:?} {path}: {body}");
            assert_eq!(body["error"]["code"], "unauthorized");
        }
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn the_tenant_header_must_name_one_of_my_tenants(pool: PgPool) {
    let app = common::app(pool).await;
    let mine = common::create_tenant(&app, "mine").await;

    for header in [None, Some("not-a-uuid")] {
        let (status, body) = common::send(&app, Method::GET, "/v1/monitors", header, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{header:?}: {body}");
        assert_eq!(body["error"]["code"], "bad_request");
    }

    let other_user = common::insert_user(&app.pool, "other@test.taut").await;
    let theirs = common::create_tenant(&app.as_user(other_user), "theirs").await;
    for tenant in [theirs.as_str(), "0192b4a8-0000-7000-8000-000000000000"] {
        let (status, body) =
            common::send(&app, Method::GET, "/v1/monitors", Some(tenant), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{tenant}: {body}");
        assert_eq!(body["error"]["code"], "not_a_member");
    }

    let (status, _) = common::send(&app, Method::GET, "/v1/monitors", Some(&mine), None).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn tenants_are_listed_per_user_with_their_role(pool: PgPool) {
    let app = common::app(pool).await;
    let acme = common::create_tenant(&app, "acme").await;
    let other_user = common::insert_user(&app.pool, "other@test.taut").await;
    let other = app.as_user(other_user);
    let beta = common::create_tenant(&other, "beta").await;
    common::add_member(&app.pool, &beta, app.user_id, "member").await;

    let (status, listed) = common::send(&app, Method::GET, "/v1/tenants", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let listed = listed.as_array().unwrap();
    assert_eq!(listed.len(), 2, "{listed:?}");
    let roles: Vec<(&str, &str)> = listed
        .iter()
        .map(|t| (t["slug"].as_str().unwrap(), t["role"].as_str().unwrap()))
        .collect();
    assert_eq!(roles, vec![("acme", "owner"), ("beta", "member")]);

    let (status, theirs) = common::send(&other, Method::GET, "/v1/tenants", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(theirs.as_array().unwrap().len(), 1);

    let (status, shown) = common::send(
        &app,
        Method::GET,
        &format!("/v1/tenants/{beta}"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(shown["role"], "member");
    let (status, _) = common::send(
        &other,
        Method::GET,
        &format!("/v1/tenants/{acme}"),
        None,
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "not mine: as if it did not exist"
    );
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn members_manage_monitors_but_not_channels(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let member_id = common::insert_user(&app.pool, "member@test.taut").await;
    common::add_member(&app.pool, &tenant, member_id, "member").await;
    let member = app.as_user(member_id);

    let monitor = common::create_monitor(&member, &tenant, http_monitor("Sitio")).await;
    let id = monitor["id"].as_str().unwrap();
    let (status, _) = common::send(
        &member,
        Method::POST,
        &format!("/v1/monitors/{id}/pause"),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let channel = json!({ "name": "Ops", "type": "email", "config": { "to": "ops@test.taut" } });
    let (status, body) = common::send(
        &member,
        Method::POST,
        "/v1/alert-channels",
        Some(&tenant),
        Some(channel.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "forbidden");

    let (status, created) = common::send(
        &app,
        Method::POST,
        "/v1/alert-channels",
        Some(&tenant),
        Some(channel),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let channel_id = created["id"].as_str().unwrap();

    let (status, _) = common::send(
        &member,
        Method::GET,
        "/v1/alert-channels",
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "members read channels");
    for (method, path) in [
        (Method::PUT, format!("/v1/alert-channels/{channel_id}")),
        (
            Method::POST,
            format!("/v1/alert-channels/{channel_id}/test"),
        ),
        (Method::DELETE, format!("/v1/alert-channels/{channel_id}")),
    ] {
        let body = (method == Method::PUT).then(|| {
            json!({ "name": "Ops", "type": "email", "config": { "to": "x@test.taut" }, "active": true })
        });
        let (status, _) = common::send(&member, method.clone(), &path, Some(&tenant), body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn members_are_listed_with_roles(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let member_id = common::insert_user(&app.pool, "member@test.taut").await;
    common::add_member(&app.pool, &tenant, member_id, "member").await;

    let (status, members) =
        common::send(&app, Method::GET, "/v1/members", Some(&tenant), None).await;
    assert_eq!(status, StatusCode::OK, "{members}");
    let members = members.as_array().unwrap();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0]["email"], "owner@test.taut");
    assert_eq!(members[0]["role"], "owner");
    assert_eq!(members[1]["email"], "member@test.taut");
    assert_eq!(members[1]["role"], "member");
    assert!(members[1]["joined_at"].is_string());
}
