//! Middleware stack shared by every HTTP service.
//!
//! From the outside in: request id (ULID, kept when a gateway already set one), a task-local
//! trace id, a tracing span per request, request-id propagation to the response, CORS, the
//! client address, HTTP metrics, panic capture and a request timeout.

use std::any::Any;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, FromRequestParts, MatchedPath, Request};
use axum::http::request::Parts;
use axum::http::{HeaderName, HeaderValue, Method, StatusCode, header};
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

/// Header that selects the tenant a request acts on. The bearer token says who the caller is;
/// this header says which of their tenants they mean, and the api checks the membership.
pub const TENANT_ID_HEADER: &str = "x-tenant-id";

/// Header a trusted proxy fills with the client address.
const FORWARDED_FOR: &str = "x-forwarded-for";

tokio::task_local! {
    static TRACE_ID: String;
}

/// Trace id of the request handled by the current task, if any.
pub fn current_trace_id() -> Option<String> {
    TRACE_ID.try_with(Clone::clone).ok()
}

/// Address of the client, as an extractor: from `X-Forwarded-For` behind a trusted proxy,
/// else the peer address of the connection. `None` only in tests that drive the router
/// in-process without a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientIp(pub Option<IpAddr>);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = Infallible;

    #[allow(clippy::unused_async_trait_impl)] // the trait's method is async
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Infallible> {
        Ok(parts
            .extensions
            .get::<ClientIp>()
            .copied()
            .unwrap_or(ClientIp(None)))
    }
}

/// Wraps `router` with the shared middleware stack.
pub fn apply(router: Router, settings: &HttpSettings) -> Router {
    let timeout = Duration::from_secs(settings.request_timeout_secs);
    let trust_proxy = settings.trust_proxy;
    if settings.cors_origins().iter().any(|origin| origin == "*") {
        tracing::warn!("cors allows any origin, so browsers will not send credentials");
    }
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
            .layer(middleware::from_fn(move |req, next| {
                client_ip(trust_proxy, req, next)
            }))
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
    // `tenant_id`, `user_id` and `api_key_id` start empty; the service records them once it
    // knows them.
    tracing::info_span!(
        "request",
        trace_id = %trace_id_from(req),
        method = %req.method(),
        path = %req.uri().path(),
        tenant_id = tracing::field::Empty,
        user_id = tracing::field::Empty,
        api_key_id = tracing::field::Empty,
    )
}

async fn client_ip(trust_proxy: bool, mut req: Request, next: Next) -> Response {
    let forwarded = trust_proxy
        .then(|| req.headers().get(FORWARDED_FOR))
        .flatten()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .and_then(|first| first.trim().parse::<IpAddr>().ok());
    let ip = forwarded.or_else(|| {
        req.extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0.ip())
    });
    req.extensions_mut().insert(ClientIp(ip));
    next.run(req).await
}

fn cors(settings: &HttpSettings) -> CorsLayer {
    let origins = settings.cors_origins();
    // Browsers only send cookies to an explicit origin; tower-http refuses `*` with
    // credentials, so the wildcard stays a credential-less CORS.
    let (allow_origin, credentials) = if origins.iter().any(|origin| origin == "*") {
        (AllowOrigin::any(), false)
    } else {
        (
            AllowOrigin::list(
                origins
                    .iter()
                    .filter_map(|origin| HeaderValue::from_str(origin).ok()),
            ),
            true,
        )
    };
    CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_credentials(credentials)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            HeaderName::from_static(TENANT_ID_HEADER),
        ])
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
