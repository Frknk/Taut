//! Middleware stack shared by every HTTP service.
//!
//! From the outside in: request id (ULID, kept when a gateway already set one), a task-local
//! trace id, a tracing span per request, request-id propagation to the response, CORS,
//! HTTP metrics, panic capture and a request timeout.

use std::any::Any;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::extract::{MatchedPath, Request};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use taut_config::HttpSettings;
use tower::ServiceBuilder;
use tower_http::LatencyUnit;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::request_id::{
    MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer,
};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::{Level, Span};
use ulid::Ulid;

use crate::error::AppError;

/// Header that carries the trace id. Incoming values are preserved so a gateway can set it.
pub const TRACE_ID_HEADER: &str = "x-request-id";

tokio::task_local! {
    static TRACE_ID: String;
}

/// Trace id of the request handled by the current task, if any.
pub fn current_trace_id() -> Option<String> {
    TRACE_ID.try_with(Clone::clone).ok()
}

/// Wraps `router` with the shared middleware stack.
pub fn apply(router: Router, settings: &HttpSettings) -> Router {
    let timeout = Duration::from_secs(settings.request_timeout_secs);
    router.layer(
        ServiceBuilder::new()
            .layer(SetRequestIdLayer::x_request_id(MakeRequestUlid))
            .layer(middleware::from_fn(trace_id_scope))
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(make_span)
                    .on_response(
                        DefaultOnResponse::new()
                            .level(Level::INFO)
                            .latency_unit(LatencyUnit::Millis),
                    ),
            )
            .layer(PropagateRequestIdLayer::x_request_id())
            .layer(cors(settings))
            .layer(middleware::from_fn(record_http_metrics))
            .layer(CatchPanicLayer::custom(panic_response))
            .layer(TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                timeout,
            )),
    )
}

#[derive(Clone, Copy, Default)]
struct MakeRequestUlid;

impl MakeRequestId for MakeRequestUlid {
    fn make_request_id<B>(&mut self, _: &http::Request<B>) -> Option<RequestId> {
        HeaderValue::from_str(&Ulid::generate().to_string())
            .ok()
            .map(RequestId::new)
    }
}

fn trace_id_from<B>(req: &http::Request<B>) -> String {
    req.headers()
        .get(TRACE_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-")
        .to_owned()
}

async fn trace_id_scope(req: Request, next: Next) -> Response {
    let trace_id = trace_id_from(&req);
    TRACE_ID.scope(trace_id, next.run(req)).await
}

fn make_span(req: &http::Request<Body>) -> Span {
    tracing::info_span!(
        "request",
        trace_id = %trace_id_from(req),
        method = %req.method(),
        path = %req.uri().path(),
    )
}

fn cors(settings: &HttpSettings) -> CorsLayer {
    let origins = settings.cors_origins();
    let allow_origin = if origins.iter().any(|origin| origin == "*") {
        AllowOrigin::any()
    } else {
        AllowOrigin::list(
            origins
                .iter()
                .filter_map(|origin| HeaderValue::from_str(origin).ok()),
        )
    };
    CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        .max_age(Duration::from_secs(600))
}

async fn record_http_metrics(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().as_str().to_owned();
    // The matched route pattern keeps label cardinality bounded; raw paths would not.
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(|| "unmatched".to_owned(), |path| path.as_str().to_owned());
    let response = next.run(req).await;
    let labels = [
        ("method", method),
        ("route", route),
        ("status", response.status().as_u16().to_string()),
    ];
    ::metrics::counter!("http_requests_total", &labels[..]).increment(1);
    ::metrics::histogram!("http_request_duration_seconds", &labels[..])
        .record(start.elapsed().as_secs_f64());
    response
}

#[allow(clippy::needless_pass_by_value)] // signature imposed by `CatchPanicLayer::custom`
fn panic_response(err: Box<dyn Any + Send + 'static>) -> Response {
    let detail = err
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_else(|| "unknown panic".to_owned());
    tracing::error!(panic = %detail, "handler panicked");
    AppError::Internal(anyhow::anyhow!("handler panicked")).into_response()
}
