//! Readiness against a real PostgreSQL (needs `DATABASE_URL`, see `.env.example`).

mod common;

use std::sync::Arc;

use http::StatusCode;
use sqlx::PgPool;
use taut_config::Settings;
use tower::ServiceExt;

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn ready_reports_database_ok(pool: PgPool) {
    let app = common::app_with(Arc::new(Settings::default()), pool).await;
    let response = app
        .router
        .clone()
        .oneshot(common::get("/ready"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = common::json(response).await;
    assert_eq!(body["status"], "ready");
    assert_eq!(body["checks"]["database"], "ok");
    assert_eq!(body["checks"]["jwks"], "ok");
}
