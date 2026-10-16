//! `/v1/tenants` against a real PostgreSQL (needs `DATABASE_URL`, see `.env.example`).

mod common;

use http::{Method, StatusCode};
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn creates_reads_and_lists_tenants(pool: PgPool) {
    let app = common::app(pool);
    let (status, created) = common::send(
        &app,
        Method::POST,
        "/v1/tenants",
        None,
        Some(json!({ "name": "  Acme  ", "slug": "Acme-Corp" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["name"], "Acme");
    assert_eq!(created["slug"], "acme-corp");
    assert_eq!(created["plan"], "free");

    let id = created["id"].as_str().unwrap();
    let (status, shown) =
        common::send(&app, Method::GET, &format!("/v1/tenants/{id}"), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(shown, created);

    let (status, listed) = common::send(&app, Method::GET, "/v1/tenants", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed.as_array().unwrap().len(), 1);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn duplicate_slug_is_a_conflict(pool: PgPool) {
    let app = common::app(pool);
    common::create_tenant(&app, "acme").await;
    let (status, body) = common::send(
        &app,
        Method::POST,
        "/v1/tenants",
        None,
        Some(json!({ "name": "Otra", "slug": "acme" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "conflict");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn invalid_input_is_a_json_bad_request(pool: PgPool) {
    let app = common::app(pool);
    for body in [
        json!({ "name": "Acme", "slug": "a" }),
        json!({ "name": "", "slug": "acme" }),
        json!({ "name": "Acme" }),
        json!({ "name": "Acme", "slug": "acme", "plan": "pro" }),
    ] {
        let (status, response) =
            common::send(&app, Method::POST, "/v1/tenants", None, Some(body.clone())).await;
        assert_eq!(status.as_u16() / 100, 4, "{body} → {response}");
        assert_eq!(
            response["error"]["code"], "bad_request",
            "{body} → {response}"
        );
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn unknown_tenant_is_not_found(pool: PgPool) {
    let app = common::app(pool);
    let path = "/v1/tenants/0192b4a8-0000-7000-8000-000000000000";
    let (status, _) = common::send(&app, Method::GET, path, None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, body) =
        common::send(&app, Method::GET, "/v1/tenants/not-a-uuid", None, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "bad_request");
}
