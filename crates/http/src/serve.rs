//! Server loop with graceful shutdown on `SIGTERM` and `Ctrl-C`.
//!
//! Kubernetes sends `SIGTERM` before killing a pod; draining in-flight requests keeps
//! rolling deployments free of spurious errors. Services that also run a background worker
//! (consumers of the bus, the scheduler) use [`serve_with_worker`], which stops the worker
//! after the server drains and exits if the worker dies on its own.

use std::future::Future;
use std::net::SocketAddr;

use axum::Router;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::watch;

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
    /// The background worker failed or stopped before shutdown was requested.
    #[error("worker failed: {0}")]
    Worker(#[source] anyhow::Error),
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

/// Runs `worker` next to the HTTP server.
///
/// The worker receives a channel that turns `true` once the server has drained; it should
/// then finish in-flight work and return. If the worker returns first, with or without an
/// error, the process should exit so the orchestrator restarts it: that is reported as
/// [`ServeError::Worker`].
pub async fn serve_with_worker<W, Fut>(
    router: Router,
    addr: SocketAddr,
    worker: W,
) -> Result<(), ServeError>
where
    W: FnOnce(watch::Receiver<bool>) -> Fut,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let (stop, stopped) = watch::channel(false);
    let mut worker = tokio::spawn(worker(stopped));
    tokio::select! {
        result = &mut worker => {
            let error = match result {
                Ok(Ok(())) => anyhow::anyhow!("worker stopped before shutdown"),
                Ok(Err(error)) => error,
                Err(join) => anyhow::anyhow!("worker panicked: {join}"),
            };
            tracing::error!(error = %error, "worker stopped, exiting");
            Err(ServeError::Worker(error))
        }
        result = serve(router, addr) => {
            stop.send_replace(true);
            tracing::info!("waiting for the worker to finish");
            let worker_result = worker.await;
            result?;
            match worker_result {
                Ok(result) => result.map_err(ServeError::Worker),
                Err(join) => Err(ServeError::Worker(anyhow::anyhow!("worker panicked: {join}"))),
            }
        }
    }
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
