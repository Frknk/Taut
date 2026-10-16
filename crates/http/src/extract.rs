//! Extractors whose rejections are [`AppError`]s.
//!
//! Axum's own extractors answer malformed input with plain-text bodies. These wrappers keep
//! the same parsing but turn every rejection into `400 Bad Request` with the project's JSON
//! error body, so clients only ever see one error format.

use axum::extract::{FromRequest, FromRequestParts};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::error::AppError;

/// JSON request body, and JSON response when returned from a handler.
#[derive(Debug, Clone, Copy, Default, FromRequest)]
#[from_request(via(axum::Json), rejection(AppError))]
pub struct Json<T>(pub T);

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

/// Path parameters.
#[derive(Debug, Clone, Copy, Default, FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(AppError))]
pub struct Path<T>(pub T);

/// Query string parameters.
#[derive(Debug, Clone, Copy, Default, FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(AppError))]
pub struct Query<T>(pub T);
