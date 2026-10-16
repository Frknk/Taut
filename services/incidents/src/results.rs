//! Writes to the `check_results` hypertable.

use sqlx::{PgConnection, PgPool};
use taut_model::events::CheckCompleted;

/// Stores `result`. Returns `false` when it was already stored (redelivery) or its monitor no
/// longer exists.
pub async fn record(pool: &PgPool, result: &CheckCompleted) -> Result<bool, sqlx::Error> {
    let mut connection = pool.acquire().await?;
    insert(&mut connection, result).await
}

/// Like [`record`], inside the caller's transaction.
pub async fn insert(
    connection: &mut PgConnection,
    result: &CheckCompleted,
) -> Result<bool, sqlx::Error> {
    let status = result.status.and_then(|status| i16::try_from(status).ok());
    let latency_ms = result
        .latency_ms
        .and_then(|latency| i32::try_from(latency).ok());
    let inserted = sqlx::query!(
        r#"
        INSERT INTO check_results (monitor_id, tenant_id, at, region, ok, status, latency_ms,
                                   error, trace_id)
        SELECT $1::uuid, $2::uuid, $3::timestamptz, $4::text, $5::boolean, $6::smallint,
               $7::integer, $8::text, $9::text
        WHERE EXISTS (SELECT 1 FROM monitors WHERE id = $1 AND tenant_id = $2)
        ON CONFLICT DO NOTHING
        "#,
        result.monitor_id,
        result.tenant_id,
        result.at,
        result.region,
        result.ok,
        status,
        latency_ms,
        result.error,
        result.trace_id,
    )
    .execute(connection)
    .await?;
    Ok(inserted.rows_affected() == 1)
}
