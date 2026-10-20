//! `/v1/tenants`: the caller's tenants.
//!
//! Creating a tenant makes the caller its owner; listing shows only the tenants the caller
//! belongs to, with their role. Everything inside a tenant hangs off `X-Tenant-Id` instead.

// Axum handlers receive their state by value even when they only borrow it.
#![allow(clippy::needless_pass_by_value)]

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use serde::Deserialize;
use taut_http::{AppError, Json, Path};
use uuid::Uuid;

use crate::AppState;
use crate::tenant::{self, CurrentUser, Plan, Role, Tenant, TenantWithRole};

const MAX_NAME_CHARS: usize = 100;

/// Routes under `/v1/tenants`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/tenants", get(list).post(create))
        .route("/v1/tenants/{id}", get(show))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NewTenant {
    name: String,
    slug: String,
}

async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(input): Json<NewTenant>,
) -> Result<(StatusCode, Json<TenantWithRole>), AppError> {
    let name = validated_name(&input.name)?;
    let slug = input.slug.trim().to_ascii_lowercase();
    if !is_valid_slug(&slug) {
        return Err(AppError::BadRequest(
            "slug must be 3 to 40 lowercase letters, digits or inner hyphens".to_owned(),
        ));
    }

    // The id comes first so the inserts already run as the new tenant.
    let id = Uuid::now_v7();
    let mut tx = state.tenant_tx(id).await?;
    let tenant = sqlx::query_as!(
        Tenant,
        r#"
        INSERT INTO tenants (id, slug, name) VALUES ($1, $2, $3)
        RETURNING id, slug, name, plan AS "plan: Plan", created_at
        "#,
        id,
        slug,
        name,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|err| {
        if err
            .as_database_error()
            .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
        {
            AppError::Conflict(format!("slug {slug} is already taken"))
        } else {
            err.into()
        }
    })?;
    sqlx::query!(
        "INSERT INTO memberships (user_id, tenant_id, role) VALUES ($1, $2, $3)",
        user.user_id,
        tenant.id,
        Role::Owner.as_str(),
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(tenant_id = %tenant.id, slug = %tenant.slug, user_id = %user.user_id, "tenant created");
    Ok((
        StatusCode::CREATED,
        Json(TenantWithRole::new(tenant, Role::Owner)),
    ))
}

async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<Vec<TenantWithRole>>, AppError> {
    // A user's tenants span several tenants by definition (owner rights).
    let mut tx = state.system_tx().await?;
    let rows = sqlx::query!(
        r#"
        SELECT t.id, t.slug, t.name, t.plan AS "plan: Plan", t.created_at, m.role AS "role: Role"
        FROM memberships m
        JOIN tenants t ON t.id = m.tenant_id
        WHERE m.user_id = $1
        ORDER BY lower(t.name), t.created_at
        "#,
        user.user_id,
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(|row| {
                TenantWithRole::new(
                    Tenant {
                        id: row.id,
                        slug: row.slug,
                        name: row.name,
                        plan: row.plan,
                        created_at: row.created_at,
                    },
                    row.role,
                )
            })
            .collect(),
    ))
}

async fn show(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TenantWithRole>, AppError> {
    tenant::find_for_member(&state.pool, id, user.user_id)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::NotFound("tenant not found".to_owned()))
}

/// The trimmed display name, 1–100 characters (the `tenants.name` CHECK).
pub(crate) fn validated_name(raw: &str) -> Result<&str, AppError> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(AppError::BadRequest(format!(
            "name must be between 1 and {MAX_NAME_CHARS} characters"
        )));
    }
    Ok(name)
}

/// Same rule as the `tenants.slug` CHECK: 3–40 characters, `[a-z0-9-]`, no hyphen at the ends.
fn is_valid_slug(slug: &str) -> bool {
    (3..=40).contains(&slug.len())
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
}

#[cfg(test)]
mod tests {
    use super::is_valid_slug;

    #[test]
    fn slugs_follow_the_database_rule() {
        for ok in ["acme", "acme-corp", "a1b", "x".repeat(40).as_str()] {
            assert!(is_valid_slug(ok), "{ok} should be valid");
        }
        for bad in [
            "ab",
            "-acme",
            "acme-",
            "Acme",
            "ac me",
            "acmé",
            "x".repeat(41).as_str(),
        ] {
            assert!(!is_valid_slug(bad), "{bad} should be invalid");
        }
    }
}
