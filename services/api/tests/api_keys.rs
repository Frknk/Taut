//! Tenant API keys: who creates them and what they may do (needs `just up`).

mod common;

use std::time::Duration;

use http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use common::TestApp;

fn http_monitor(name: &str) -> Value {
    json!({ "name": name, "type": "http", "target": "https://example.com/health" })
}

/// Creates a key as the app's user (an owner with a fresh token) and returns the body.
async fn create_key(app: &TestApp, tenant: &str, scope: &str) -> Value {
    let (status, body) = common::send(
        app,
        Method::POST,
        "/v1/api-keys",
        Some(tenant),
        Some(json!({ "name": format!("CI {scope}"), "scope": scope })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body
}

/// Sends a request with an API key as the bearer.
async fn with_key(
    app: &TestApp,
    key: &str,
    method: Method,
    path: &str,
    tenant: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    common::send_raw(app, method, path, Some(key), tenant, body).await
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn owners_create_list_and_revoke_keys(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;

    let created = create_key(&app, &tenant, "read").await;
    let key = created["key"].as_str().unwrap();
    assert!(key.starts_with("taut_live_"), "{created}");
    assert!(key.starts_with(created["prefix"].as_str().unwrap()));
    assert_eq!(created["name"], "CI read");
    assert_eq!(created["scope"], "read");
    assert_eq!(created["last_used_at"], Value::Null);

    let stored: String = sqlx::query_scalar("SELECT key_hash FROM api_keys WHERE id = $1::uuid")
        .bind(created["id"].as_str().unwrap())
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_ne!(stored, key, "only the hash is stored");

    let (status, listed) =
        common::send(&app, Method::GET, "/v1/api-keys", Some(&tenant), None).await;
    assert_eq!(status, StatusCode::OK);
    let listed = listed.as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["id"], created["id"]);
    assert!(listed[0].get("key").is_none(), "the key is shown only once");
    assert!(!listed[0].to_string().contains(key));

    let path = format!("/v1/api-keys/{}", created["id"].as_str().unwrap());
    let (status, _) = common::send(&app, Method::DELETE, &path, Some(&tenant), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, listed) = common::send(&app, Method::GET, "/v1/api-keys", Some(&tenant), None).await;
    assert!(listed[0]["revoked_at"].is_string(), "{listed}");

    let missing = format!("/v1/api-keys/{}", Uuid::now_v7());
    let (status, _) = common::send(&app, Method::DELETE, &missing, Some(&tenant), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn creating_a_key_needs_an_owner_with_a_recent_authentication(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let body = json!({ "name": "CI", "scope": "write" });

    let member_id = common::insert_user(&app.pool, "member@test.taut").await;
    common::add_member(&app.pool, &tenant, member_id, "member").await;
    let member = app.as_user(member_id);
    for (method, body) in [(Method::POST, Some(body.clone())), (Method::GET, None)] {
        let (status, answer) =
            common::send(&member, method.clone(), "/v1/api-keys", Some(&tenant), body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method}: {answer}");
        assert_eq!(answer["error"]["code"], "forbidden");
    }

    let stale = TestApp {
        token: common::token_authenticated_ago(&app.keys, app.user_id, 600),
        ..app.as_user(app.user_id)
    };
    let (status, answer) = common::send(
        &stale,
        Method::POST,
        "/v1/api-keys",
        Some(&tenant),
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    assert_eq!(answer["error"]["code"], "reauth_required");

    for invalid in [
        json!({ "name": " ", "scope": "read" }),
        json!({ "name": "x".repeat(101), "scope": "read" }),
        json!({ "name": "CI", "scope": "admin" }),
    ] {
        let (status, answer) = common::send(
            &app,
            Method::POST,
            "/v1/api-keys",
            Some(&tenant),
            Some(invalid.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{invalid}: {answer}");
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn a_tenant_has_at_most_twenty_active_keys(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let mut first = Value::Null;
    for i in 0..20 {
        let created = create_key(&app, &tenant, "read").await;
        if i == 0 {
            first = created;
        }
    }
    let body = json!({ "name": "one too many", "scope": "read" });
    let (status, answer) = common::send(
        &app,
        Method::POST,
        "/v1/api-keys",
        Some(&tenant),
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");

    let path = format!("/v1/api-keys/{}", first["id"].as_str().unwrap());
    common::send(&app, Method::DELETE, &path, Some(&tenant), None).await;
    let (status, _) = common::send(
        &app,
        Method::POST,
        "/v1/api-keys",
        Some(&tenant),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "revoked keys do not count");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn read_keys_only_read(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let monitor = common::create_monitor(&app, &tenant, http_monitor("API")).await;
    let key = create_key(&app, &tenant, "read").await["key"]
        .as_str()
        .unwrap()
        .to_owned();

    // The key carries its tenant: the header is optional.
    for header in [None, Some(tenant.as_str())] {
        let (status, listed) =
            with_key(&app, &key, Method::GET, "/v1/monitors", header, None).await;
        assert_eq!(status, StatusCode::OK, "{listed}");
        assert_eq!(listed.as_array().unwrap().len(), 1);
    }
    let id = monitor["id"].as_str().unwrap();
    for path in [
        format!("/v1/monitors/{id}"),
        format!("/v1/monitors/{id}/checks"),
        format!("/v1/monitors/{id}/metrics"),
        "/v1/incidents".to_owned(),
    ] {
        let (status, body) = with_key(&app, &key, Method::GET, &path, None, None).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
    }

    let pause = format!("/v1/monitors/{id}/pause");
    let remove = format!("/v1/monitors/{id}");
    for (method, path, body) in [
        (Method::POST, "/v1/monitors", Some(http_monitor("x"))),
        (Method::POST, pause.as_str(), None),
        (Method::DELETE, remove.as_str(), None),
    ] {
        let (status, answer) = with_key(&app, &key, method.clone(), path, None, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}: {answer}");
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn write_keys_manage_monitors_but_never_the_tenant(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let key = create_key(&app, &tenant, "write").await["key"]
        .as_str()
        .unwrap()
        .to_owned();

    let (status, created) = with_key(
        &app,
        &key,
        Method::POST,
        "/v1/monitors",
        None,
        Some(http_monitor("from CI")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let pause = format!("/v1/monitors/{}/pause", created["id"].as_str().unwrap());
    let (status, _) = with_key(&app, &key, Method::POST, &pause, None, None).await;
    assert_eq!(status, StatusCode::OK);

    let channel = json!({ "name": "Ops", "type": "email", "config": { "to": "ops@acme.test" } });
    for (method, path, body) in [
        (Method::GET, "/v1/members", None),
        (Method::GET, "/v1/alert-channels", None),
        (Method::POST, "/v1/alert-channels", Some(channel)),
        (Method::GET, "/v1/api-keys", None),
        (
            Method::POST,
            "/v1/api-keys",
            Some(json!({ "name": "more", "scope": "write" })),
        ),
    ] {
        let (status, answer) = with_key(&app, &key, method.clone(), path, None, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}: {answer}");
        assert_eq!(
            answer["error"]["message"], "api keys cannot manage the tenant",
            "{method} {path}"
        );
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn keys_are_bound_to_their_tenant_and_not_to_user_routes(pool: PgPool) {
    let app = common::app(pool).await;
    let acme = common::create_tenant(&app, "acme").await;
    let beta = common::create_tenant(&app, "beta").await;
    let key = create_key(&app, &acme, "write").await["key"]
        .as_str()
        .unwrap()
        .to_owned();

    let (status, body) = with_key(&app, &key, Method::GET, "/v1/monitors", Some(&beta), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, _) = with_key(
        &app,
        &key,
        Method::GET,
        "/v1/monitors",
        Some("not-a-uuid"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let show = format!("/v1/tenants/{acme}");
    for path in ["/v1/tenants", show.as_str()] {
        let (status, body) = with_key(&app, &key, Method::GET, path, None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}: {body}");
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn revoked_and_unknown_keys_are_unauthorized(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let created = create_key(&app, &tenant, "read").await;
    let key = created["key"].as_str().unwrap();
    let (status, _) = with_key(&app, key, Method::GET, "/v1/monitors", None, None).await;
    assert_eq!(status, StatusCode::OK);

    let path = format!("/v1/api-keys/{}", created["id"].as_str().unwrap());
    common::send(&app, Method::DELETE, &path, Some(&tenant), None).await;
    let unknown = format!("{}x", &key[..key.len() - 1]);
    for key in [key, unknown.as_str()] {
        let (status, body) = with_key(&app, key, Method::GET, "/v1/monitors", None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
        assert_eq!(body["error"]["code"], "unauthorized");
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn using_a_key_records_when(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let created = create_key(&app, &tenant, "read").await;
    let id: Uuid = created["id"].as_str().unwrap().parse().unwrap();
    let key = created["key"].as_str().unwrap();

    let (status, _) = with_key(&app, key, Method::GET, "/v1/monitors", None, None).await;
    assert_eq!(status, StatusCode::OK);

    // The write happens off the request: wait for it.
    let mut last_used = None;
    for _ in 0..50 {
        last_used = sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
            "SELECT last_used_at FROM api_keys WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
        if last_used.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let first = last_used.expect("last_used_at is recorded");

    // A second use within the minute does not write again.
    with_key(&app, key, Method::GET, "/v1/monitors", None, None).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (_, listed) = common::send(&app, Method::GET, "/v1/api-keys", Some(&tenant), None).await;
    let listed_at: chrono::DateTime<chrono::Utc> =
        serde_json::from_value(listed[0]["last_used_at"].clone()).unwrap();
    assert_eq!(listed_at, first);
}
