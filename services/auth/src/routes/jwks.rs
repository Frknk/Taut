//! `GET /.well-known/jwks.json`: the public key the other services verify tokens with.

use axum::extract::State;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use taut_http::Json;

use crate::AppState;

/// Cached a few minutes: a rotated key shows up at the next fetch, and the api also refetches
/// on its own when a token names a key it does not have.
pub(crate) async fn jwks(State(state): State<AppState>) -> Response {
    let mut response = Json(state.keys.jwks()).into_response();
    response.headers_mut().insert(
        CACHE_CONTROL,
        "public, max-age=300".parse().expect("static header"),
    );
    response
}
