//! Server loop with graceful shutdown on `SIGTERM` and `Ctrl-C`.
//!
//! Kubernetes sends `SIGTERM` before killing a pod; draining in-flight requests keeps
//! rolling deployments free of spurious errors.

use std::net::SocketAddr;

use axum::Router;
use tokio::net::TcpListener;
use tokio::signal;

/// Failure to bind or run the server.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    /// The socket could not be bound.
    #[error("failed to bind {addr}: {source}")]
    Bind {
        /// Address that was requested.
        addr: SocketAddr,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The server loop failed.
    #[error("server error: {0}")]
    Io(#[from] std::io::Error),
}

/// Serves `router` on `addr` until a shutdown signal arrives, then drains connections.
pub async fn serve(router: Router, addr: SocketAddr) -> Result<(), ServeError> {
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|source| ServeError::Bind { addr, source })?;
    tracing::info!(%addr, "listening");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("server stopped");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received, draining connections");
}
