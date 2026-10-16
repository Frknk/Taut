//! `/v1/tenants`: create, list and read tenants.
//!
//! Listing every tenant is provisional: without authentication the panel needs it to pick a
//! tenant. When JWTs arrive (weeks 13–14) the list goes away and a user sees only their own.

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
use crate::tenant::{self, Plan, Tenant};

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
    Json(input): Json<NewTenant>,
) -> Result<(StatusCode, Json<Tenant>), AppError> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(AppError::BadRequest(format!(
            "name must be between 1 and {MAX_NAME_CHARS} characters"
        )));
    }
    let slug = input.slug.trim().to_ascii_lowercase();
    if !is_valid_slug(&slug) {
        return Err(AppError::BadRequest(
            "slug must be 3 to 40 lowercase letters, digits or inner hyphens".to_owned(),
        ));
    }

    let tenant = sqlx::query_as!(
        Tenant,
        r#"
        INSERT INTO tenants (id, slug, name) VALUES ($1, $2, $3)
        RETURNING id, slug, name, plan AS "plan: Plan", created_at
        "#,
        Uuid::now_v7(),
        slug,
        name,
    )
    .fetch_one(&state.pool)
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
    tracing::info!(tenant_id = %tenant.id, slug = %tenant.slug, "tenant created");
    Ok((StatusCode::CREATED, Json(tenant)))
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<Tenant>>, AppError> {
    let tenants = sqlx::query_as!(
        Tenant,
        r#"SELECT id, slug, name, plan AS "plan: Plan", created_at FROM tenants ORDER BY created_at"#
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(tenants))
}

async fn show(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Tenant>, AppError> {
    tenant::find(&state.pool, id)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::NotFound("tenant not found".to_owned()))
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
