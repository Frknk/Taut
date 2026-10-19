//! Row-level security per tenant against a real PostgreSQL with TimescaleDB (`just up`).
//!
//! Every test seeds two tenants with one row in each tenant table, through the owner pool the
//! test gets, and checks what the app role can see and write.

use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, Executor, PgPool, Postgres};
use uuid::Uuid;

/// The tables row-level security protects, `tenants` first.
const TENANT_TABLES: [&str; 10] = [
    "tenants",
    "memberships",
    "invitations",
    "api_keys",
    "monitors",
    "monitor_status",
    "check_results",
    "incidents",
    "alert_channels",
    "notification_deliveries",
];

/// A tenant with one row in every tenant table; returns its id.
async fn seed(pool: &PgPool, slug: &str) -> Uuid {
    let tenant = Uuid::now_v7();
    let user = Uuid::now_v7();
    let monitor = Uuid::now_v7();
    let incident = Uuid::now_v7();
    let channel = Uuid::now_v7();
    let statements = [
        "INSERT INTO tenants (id, slug, name) VALUES ($1, $5, 'Tenant')",
        "INSERT INTO users (id, email, name, password_hash) VALUES ($2, $5 || '@taut.test', 'User', 'x')",
        "INSERT INTO memberships (user_id, tenant_id, role) VALUES ($2, $1, 'owner')",
        "INSERT INTO invitations (id, tenant_id, email, role, token_hash, expires_at)
         VALUES ($2, $1, 'guest@' || $5 || '.test', 'member', 'hash-' || $5, now() + interval '7 days')",
        "INSERT INTO api_keys (id, tenant_id, name, prefix, key_hash, scope)
         VALUES ($2, $1, 'CI', 'taut_live_x', 'key-' || $5, 'read')",
        "INSERT INTO monitors (id, tenant_id, name, kind, target)
         VALUES ($3, $1, 'API', 'http', 'https://example.com/')",
        "INSERT INTO monitor_status (monitor_id, tenant_id) VALUES ($3, $1)",
        "INSERT INTO check_results (monitor_id, tenant_id, at, region, ok, latency_ms)
         VALUES ($3, $1, now() - interval '2 hours', 'local', true, 42)",
        "INSERT INTO incidents (id, tenant_id, monitor_id, started_at, cause)
         VALUES ($4, $1, $3, now(), 'down')",
        "INSERT INTO alert_channels (id, tenant_id, name, kind, config)
         VALUES ($6, $1, 'Ops', 'email', '{\"to\": \"ops@taut.test\"}')",
        "INSERT INTO notification_deliveries (incident_id, channel_id, event, tenant_id, status)
         VALUES ($4, $6, 'opened', $1, 'sent')",
    ];
    for statement in statements {
        sqlx::query(statement)
            .bind(tenant)
            .bind(user)
            .bind(monitor)
            .bind(incident)
            .bind(slug)
            .bind(channel)
            .execute(pool)
            .await
            .unwrap_or_else(|err| panic!("{statement}: {err}"));
    }
    tenant
}

async fn count<'c, E>(executor: E, table: &str) -> i64
where
    E: Executor<'c, Database = Postgres>,
{
    // The table names are the constants above, never input.
    sqlx::query_scalar(AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(executor)
        .await
        .unwrap_or_else(|err| panic!("count {table}: {err}"))
}

/// SQLSTATE of a database error.
fn code(err: &sqlx::Error) -> Option<String> {
    err.as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .map(std::borrow::Cow::into_owned)
}

/// SQLSTATE `insufficient_privilege`: a policy or a missing grant refused the statement.
const DENIED: &str = "42501";

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn the_restricted_pool_acts_as_the_app_role(pool: PgPool) {
    let restricted = taut_db::restricted(&pool);
    let (user, superuser, bypass): (String, bool, bool) = sqlx::query_as(
        "SELECT current_user::text, rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(&restricted)
    .await
    .unwrap();
    assert_eq!(user, taut_db::APP_ROLE);
    assert!(!superuser);
    assert!(!bypass);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn queries_outside_a_tenant_transaction_see_nothing(pool: PgPool) {
    let acme = seed(&pool, "acme").await;
    let restricted = taut_db::restricted(&pool);
    for table in TENANT_TABLES {
        assert_eq!(count(&restricted, table).await, 0, "{table}");
    }
    let err = sqlx::query("INSERT INTO alert_channels (id, tenant_id, name, kind, config) VALUES ($1, $2, 'x', 'email', '{}')")
        .bind(Uuid::now_v7())
        .bind(acme)
        .execute(&restricted)
        .await
        .unwrap_err();
    assert_eq!(code(&err).as_deref(), Some(DENIED), "{err}");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn a_tenant_transaction_only_sees_its_tenant(pool: PgPool) {
    let acme = seed(&pool, "acme").await;
    let globex = seed(&pool, "globex").await;
    let restricted = taut_db::restricted(&pool);

    for tenant in [acme, globex] {
        let mut tx = taut_db::begin_tenant(&restricted, tenant).await.unwrap();
        for table in TENANT_TABLES {
            assert_eq!(count(&mut *tx, table).await, 1, "{table}");
        }
        let seen: Uuid = sqlx::query_scalar("SELECT id FROM tenants")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(seen, tenant);
        tx.commit().await.unwrap();
    }
    // The owner still sees both.
    for table in TENANT_TABLES {
        assert_eq!(count(&pool, table).await, 2, "{table}");
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn writes_for_another_tenant_are_rejected(pool: PgPool) {
    let acme = seed(&pool, "acme").await;
    let globex = seed(&pool, "globex").await;
    let restricted = taut_db::restricted(&pool);
    let mut tx = taut_db::begin_tenant(&restricted, acme).await.unwrap();

    let updated = sqlx::query("UPDATE monitors SET name = 'Hijacked' WHERE tenant_id = $1")
        .bind(globex)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(updated.rows_affected(), 0);
    let deleted = sqlx::query("DELETE FROM alert_channels WHERE tenant_id = $1")
        .bind(globex)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(deleted.rows_affected(), 0);
    // Moving a row to another tenant fails the policy's check.
    let err = sqlx::query("UPDATE monitors SET tenant_id = $1")
        .bind(globex)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(code(&err).as_deref(), Some(DENIED), "{err}");
    tx.rollback().await.unwrap();

    let mut tx = taut_db::begin_tenant(&restricted, acme).await.unwrap();
    let err = sqlx::query(
        "INSERT INTO monitors (id, tenant_id, name, kind, target) VALUES ($1, $2, 'x', 'http', 'https://example.com/')",
    )
    .bind(Uuid::now_v7())
    .bind(globex)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(code(&err).as_deref(), Some(DENIED), "{err}");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn the_tenant_does_not_leak_to_the_next_checkout(pool: PgPool) {
    seed(&pool, "globex").await;
    let acme = seed(&pool, "acme").await;
    // One connection, so the second checkout reuses the first one's session.
    let single = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let restricted = taut_db::restricted(&single);

    let mut tx = taut_db::begin_tenant(&restricted, acme).await.unwrap();
    assert_eq!(count(&mut *tx, "monitors").await, 1);
    tx.commit().await.unwrap();

    let (user, setting): (String, String) =
        sqlx::query_as("SELECT current_user::text, current_setting('app.tenant_id', true)")
            .fetch_one(&restricted)
            .await
            .unwrap();
    assert_eq!(user, taut_db::APP_ROLE);
    assert_eq!(setting, "");
    assert_eq!(count(&restricted, "monitors").await, 0);
    restricted.close().await;
    single.close().await;
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn begin_tenant_restricts_an_owner_pool_too(pool: PgPool) {
    let acme = seed(&pool, "acme").await;
    seed(&pool, "globex").await;
    let mut tx = taut_db::begin_tenant(&pool, acme).await.unwrap();
    let user: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(user, taut_db::APP_ROLE);
    assert_eq!(count(&mut *tx, "incidents").await, 1);
    tx.commit().await.unwrap();
    assert_eq!(count(&pool, "incidents").await, 2);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn begin_system_sees_every_tenant(pool: PgPool) {
    seed(&pool, "acme").await;
    seed(&pool, "globex").await;
    let restricted = taut_db::restricted(&pool);
    let mut tx = taut_db::begin_system(&restricted).await.unwrap();
    assert_eq!(count(&mut *tx, "api_keys").await, 2);
    tx.commit().await.unwrap();
    assert_eq!(count(&restricted, "api_keys").await, 0);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn inserts_that_create_a_chunk_work_as_the_app_role(pool: PgPool) {
    let acme = seed(&pool, "acme").await;
    let monitor: Uuid = sqlx::query_scalar("SELECT id FROM monitors WHERE tenant_id = $1")
        .bind(acme)
        .fetch_one(&pool)
        .await
        .unwrap();
    let restricted = taut_db::restricted(&pool);
    let mut tx = taut_db::begin_tenant(&restricted, acme).await.unwrap();
    // A day no chunk covers yet: the insert creates one.
    sqlx::query(
        "INSERT INTO check_results (monitor_id, tenant_id, at, region, ok)
         VALUES ($1, $2, '2001-01-01T00:00:00Z', 'local', false)",
    )
    .bind(monitor)
    .bind(acme)
    .execute(&mut *tx)
    .await
    .unwrap();
    assert_eq!(count(&mut *tx, "check_results").await, 2);
    tx.commit().await.unwrap();
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn every_tenant_table_has_row_level_security(pool: PgPool) {
    // Every table of the public schema with a tenant_id column (and tenants itself) must have
    // RLS on, the tenant_isolation policy and grants for the app role. A new tenant table that
    // misses any of them fails here.
    let tables: Vec<(String, bool, bool, bool)> = sqlx::query_as(
        r"
        SELECT c.relname::text,
               c.relrowsecurity,
               EXISTS (SELECT 1 FROM pg_policies p
                       WHERE p.schemaname = 'public' AND p.tablename = c.relname
                         AND p.policyname = 'tenant_isolation'),
               has_table_privilege('taut_app', c.oid, 'SELECT, INSERT, UPDATE, DELETE')
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'public' AND c.relkind IN ('r', 'p')
          AND (c.relname = 'tenants'
               OR EXISTS (SELECT 1 FROM pg_attribute a
                          WHERE a.attrelid = c.oid AND a.attname = 'tenant_id'
                            AND NOT a.attisdropped))
        ORDER BY 1
        ",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let mut names: Vec<&str> = tables.iter().map(|(name, ..)| name.as_str()).collect();
    names.sort_unstable();
    let mut expected = TENANT_TABLES.to_vec();
    expected.sort_unstable();
    assert_eq!(names, expected);
    for (name, rls, policy, granted) in &tables {
        assert!(rls, "{name} has row-level security off");
        assert!(policy, "{name} has no tenant_isolation policy");
        assert!(granted, "{name} is not granted to taut_app");
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn uptime_is_read_only_through_the_tenant_function(pool: PgPool) {
    let acme = seed(&pool, "acme").await;
    seed(&pool, "globex").await;
    // The refresh still works with RLS on the hypertable; it runs as the owner.
    sqlx::query("CALL refresh_continuous_aggregate('uptime_daily', NULL, NULL)")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(count(&pool, "uptime_daily").await, 2);

    let restricted = taut_db::restricted(&pool);
    let mut tx = taut_db::begin_tenant(&restricted, acme).await.unwrap();
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM uptime_daily_of_current_tenant(now() - interval '3 days')",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(rows, 1);
    let err = sqlx::query("SELECT count(*) FROM uptime_daily")
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(code(&err).as_deref(), Some(DENIED), "{err}");
    tx.rollback().await.unwrap();

    // Outside a tenant transaction the function returns nothing.
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM uptime_daily_of_current_tenant(now() - interval '3 days')",
    )
    .fetch_one(&restricted)
    .await
    .unwrap();
    assert_eq!(rows, 0);
}
