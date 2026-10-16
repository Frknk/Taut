//! HTTP building blocks shared by every taut service.
//!
//! A service composes its router from these pieces:
//!
//! ```ignore
//! let router = Router::new()
//!     .merge(taut_http::health::router(info, pool))
//!     .merge(taut_http::metrics::router(handle))
//!     .merge(my_routes);
//! let router = taut_http::layers::apply(router, &settings.http);
//! taut_http::serve(router, settings.http.addr).await?;
//! ```

pub mod error;
pub mod extract;
pub mod health;
pub mod layers;
pub mod metrics;
pub mod serve;

pub use error::AppError;
pub use extract::{Json, Path, Query};
pub use health::ServiceInfo;
pub use layers::current_trace_id;
pub use serve::{ServeError, serve};
