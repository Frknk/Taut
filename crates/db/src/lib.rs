//! PostgreSQL access for Taut services: connection pool, migrations, health ping and the
//! tenant-scoped transactions of row-level security.
//!
//! All services share one database in the MVP, so the migrations live at the repository
//! root (`migrations/`) and are embedded here through [`MIGRATOR`]. Integration tests use
//! `#[sqlx::test(migrator = "taut_db::MIGRATOR")]` to get a fresh, migrated database.
//!
//! Row-level security (ADR 0010): every tenant table has a policy for [`APP_ROLE`] that only
//! lets through the rows of the transaction's tenant. [`begin_tenant`] opens such a
//! transaction; [`restricted`] makes a pool whose connections act as that role from the start,
//! so a query outside a tenant transaction sees nothing; [`begin_system`] is the explicit way
//! out, for the few lookups that find the tenant in the first place.

use std::time::Duration;

use sqlx::migrate::{MigrateError, Migrator};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres, Transaction};
use taut_config::DatabaseSettings;
use uuid::Uuid;

/// Every migration under the repository's `migrations/` directory, embedded at build time.
pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// Role the services act as on behalf of a tenant: no login, no `BYPASSRLS`, privileges on
/// the tenant tables only. Created by the `row_level_security` migration.
pub const APP_ROLE: &str = "taut_app";

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

/// A pool on the same database as `owner` whose connections act as [`APP_ROLE`] from the
/// start: outside a [`begin_tenant`] transaction its queries see no tenant rows and writes to
/// tenant tables fail. Same size and timeouts as `owner`; it connects on first use.
pub fn restricted(owner: &PgPool) -> PgPool {
    let options = owner
        .connect_options()
        .as_ref()
        .clone()
        .options([("role", APP_ROLE)]);
    PgPoolOptions::new()
        .max_connections(owner.options().get_max_connections())
        .acquire_timeout(owner.options().get_acquire_timeout())
        .connect_lazy_with(options)
}

/// Opens a transaction that acts as [`APP_ROLE`] for `tenant_id`: row-level security lets
/// through only that tenant's rows, and rejects writes for any other. Works on any pool; both
/// settings end with the transaction.
pub async fn begin_tenant(
    pool: &PgPool,
    tenant_id: Uuid,
) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('role', $1, true), set_config('app.tenant_id', $2, true)")
        .bind(APP_ROLE)
        .bind(tenant_id.to_string())
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

/// Opens a transaction with the owner's rights, which bypass row-level security. Only for
/// lookups that find the tenant in the first place (an API key by its hash, a heartbeat by
/// its token, an invitation by its link) or that span a user's tenants.
pub async fn begin_system(pool: &PgPool) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('role', 'none', true)")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}
