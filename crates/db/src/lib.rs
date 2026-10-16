//! PostgreSQL access for taut services: connection pool, migrations and health ping.
//!
//! All services share one database in the MVP, so the migrations live at the repository
//! root (`migrations/`) and are embedded here through [`MIGRATOR`]. Integration tests use
//! `#[sqlx::test(migrator = "taut_db::MIGRATOR")]` to get a fresh, migrated database.

use std::time::Duration;

use sqlx::PgPool;
use sqlx::migrate::{MigrateError, Migrator};
use sqlx::postgres::PgPoolOptions;
use taut_config::DatabaseSettings;

/// Every migration under the repository's `migrations/` directory, embedded at build time.
pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// Database failure.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// The pool could not open its first connection.
    #[error("database connection failed: {0}")]
    Connect(#[source] sqlx::Error),
    /// A migration failed to apply.
    #[error("database migration failed: {0}")]
    Migrate(#[from] MigrateError),
    /// The health ping failed.
    #[error("database ping failed: {0}")]
    Ping(#[source] sqlx::Error),
}

/// Opens a connection pool and verifies that the database is reachable.
pub async fn connect(settings: &DatabaseSettings) -> Result<PgPool, DbError> {
    let pool = PgPoolOptions::new()
        .max_connections(settings.max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&settings.url)
        .await
        .map_err(DbError::Connect)?;
    tracing::info!(
        max_connections = settings.max_connections,
        "database pool ready"
    );
    Ok(pool)
}

/// Applies all pending migrations.
pub async fn migrate(pool: &PgPool) -> Result<(), DbError> {
    MIGRATOR.run(pool).await?;
    tracing::info!("database migrations applied");
    Ok(())
}

/// Runs a trivial query to confirm the database answers.
pub async fn ping(pool: &PgPool) -> Result<(), DbError> {
    sqlx::query("SELECT 1")
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(DbError::Ping)
}
