//! Integration tests against a real PostgreSQL (needs `DATABASE_URL`, see `.env.example`).

use sqlx::PgPool;

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn migrations_install_timescaledb(pool: PgPool) {
    let installed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'timescaledb')",
    )
    .fetch_one(&pool)
    .await
    .expect("query runs");
    assert!(
        installed,
        "timescaledb extension must exist after migrations"
    );
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn ping_succeeds_on_live_pool(pool: PgPool) {
    taut_db::ping(&pool).await.expect("ping succeeds");
}
