//! Roles, removing and leaving, and the tenant's own lifecycle (needs `just up`).

mod common;

use http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn member_path(user_id: Uuid) -> String {
    format!("/v1/members/{user_id}")
}

async fn role_change(
    app: &common::TestApp,
    tenant: &str,
    user_id: Uuid,
    role: &str,
) -> (StatusCode, Value) {
    common::send(
        app,
        Method::PUT,
        &member_path(user_id),
        Some(tenant),
        Some(json!({ "role": role })),
    )
    .await
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn the_last_owner_cannot_be_demoted_removed_or_leave(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;

    let (status, body) = role_change(&app, &tenant, app.user_id, "member").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["error"]["message"],
        "the tenant needs at least one owner"
    );
    let (status, _) = common::send(
        &app,
        Method::DELETE,
        &member_path(app.user_id),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) =
        common::send(&app, Method::POST, "/v1/tenant/leave", Some(&tenant), None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // With a second owner, the first one can step down, and then leave.
    let ana = common::insert_user(&app.pool, "ana@test.taut").await;
    common::add_member(&app.pool, &tenant, ana, "owner").await;
    let (status, body) = role_change(&app, &tenant, app.user_id, "member").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["role"], "member");
    let ana = app.as_user(ana);
    let (status, _) = role_change(&ana, &tenant, ana.user_id, "member").await;
    assert_eq!(status, StatusCode::CONFLICT, "ana is now the only owner");
    let (status, _) =
        common::send(&app, Method::POST, "/v1/tenant/leave", Some(&tenant), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body) = common::send(&app, Method::GET, "/v1/tenant", Some(&tenant), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "not_a_member");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn owners_change_roles_and_remove_members(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let max = common::insert_user(&app.pool, "max@test.taut").await;
    let lea = common::insert_user(&app.pool, "lea@test.taut").await;
    common::add_member(&app.pool, &tenant, max, "member").await;
    common::add_member(&app.pool, &tenant, lea, "member").await;

    // Members cannot manage other members.
    let as_max = app.as_user(max);
    let (status, _) = role_change(&as_max, &tenant, lea, "owner").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = common::send(
        &as_max,
        Method::DELETE,
        &member_path(lea),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, body) = role_change(&app, &tenant, max, "owner").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["email"], "max@test.taut");
    assert_eq!(body["role"], "owner");

    let (status, _) =
        common::send(&app, Method::DELETE, &member_path(lea), Some(&tenant), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body) = common::send(
        &app.as_user(lea),
        Method::GET,
        "/v1/monitors",
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "not_a_member");

    // Someone who is not a member, or not a user at all.
    for user_id in [lea, Uuid::now_v7()] {
        let (status, _) = role_change(&app, &tenant, user_id, "owner").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = common::send(
            &app,
            Method::DELETE,
            &member_path(user_id),
            Some(&tenant),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn members_can_leave(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let max = common::insert_user(&app.pool, "max@test.taut").await;
    common::add_member(&app.pool, &tenant, max, "member").await;
    let max = app.as_user(max);

    let (status, _) =
        common::send(&max, Method::POST, "/v1/tenant/leave", Some(&tenant), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, tenants) = common::send(&max, Method::GET, "/v1/tenants", None, None).await;
    assert_eq!(tenants, json!([]));
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn owners_rename_the_tenant(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;

    let (status, body) = common::send(
        &app,
        Method::PUT,
        "/v1/tenant",
        Some(&tenant),
        Some(json!({ "name": "  Acme Corp  " })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "Acme Corp");
    assert_eq!(body["slug"], "acme");
    let (_, shown) = common::send(&app, Method::GET, "/v1/tenant", Some(&tenant), None).await;
    assert_eq!(shown["name"], "Acme Corp");

    let (status, _) = common::send(
        &app,
        Method::PUT,
        "/v1/tenant",
        Some(&tenant),
        Some(json!({ "name": "   " })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let max = common::insert_user(&app.pool, "max@test.taut").await;
    common::add_member(&app.pool, &tenant, max, "member").await;
    let (status, _) = common::send(
        &app.as_user(max),
        Method::PUT,
        "/v1/tenant",
        Some(&tenant),
        Some(json!({ "name": "Mine now" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

async fn count(pool: &PgPool, table: &str, tenant: &str) -> i64 {
    let column = if table == "tenants" {
        "id"
    } else {
        "tenant_id"
    };
    // Table names come from the test itself, never from input.
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM {table} WHERE {column} = $1::uuid"
    )))
    .bind(tenant)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn deleting_the_tenant_removes_everything(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let other = common::create_tenant(&app, "other").await;
    let max = common::insert_user(&app.pool, "max@test.taut").await;
    common::add_member(&app.pool, &tenant, max, "member").await;
    for tenant in [&tenant, &other] {
        let monitor = common::create_monitor(
            &app,
            tenant,
            json!({ "name": "API", "type": "http", "target": "https://example.com" }),
        )
        .await;
        sqlx::query(
            "INSERT INTO check_results (monitor_id, tenant_id, at, region, ok)
             VALUES ($1::uuid, $2::uuid, now(), 'local', true)",
        )
        .bind(monitor["id"].as_str().unwrap())
        .bind(tenant)
        .execute(&app.pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO invitations (id, tenant_id, email, role, token_hash, expires_at)
         VALUES ($1, $2::uuid, 'bea@test.taut', 'member', 'hash', now() + interval '1 day')",
    )
    .bind(Uuid::now_v7())
    .bind(&tenant)
    .execute(&app.pool)
    .await
    .unwrap();

    // Members cannot, and owners need a recent authentication.
    let (status, _) = common::send(
        &app.as_user(max),
        Method::DELETE,
        "/v1/tenant",
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let stale = common::token_authenticated_ago(&app.keys, app.user_id, 600);
    let (status, body) = common::send_raw(
        &app,
        Method::DELETE,
        "/v1/tenant",
        Some(&stale),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "reauth_required");
    assert_eq!(count(&app.pool, "tenants", &tenant).await, 1);

    let (status, body) =
        common::send(&app, Method::DELETE, "/v1/tenant", Some(&tenant), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    for table in [
        "tenants",
        "check_results",
        "monitors",
        "memberships",
        "invitations",
    ] {
        assert_eq!(count(&app.pool, table, &tenant).await, 0, "{table}");
    }
    // The other tenant is untouched.
    assert_eq!(count(&app.pool, "check_results", &other).await, 1);
    assert_eq!(count(&app.pool, "memberships", &other).await, 1);

    let (status, body) = common::send(&app, Method::GET, "/v1/tenant", Some(&tenant), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "not_a_member");
}
