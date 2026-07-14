//! HTTP server assembly.

mod cors;

use std::net::SocketAddr;

use axum::Router;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::api;
use crate::models::{AppState, Config};

/// Build the fully-configured Axum [`Router`].
pub fn build_app(state: AppState, cfg: &Config) -> Router {
    let limit = cfg.body_limit_bytes;
    api::router(state)
        .layer(RequestBodyLimitLayer::new(limit))
        .layer(TraceLayer::new_for_http())
        .layer(cors::layer(cfg))
}

/// Bind and serve forever. Returns only on error or shutdown.
pub async fn serve(state: AppState, cfg: &Config) -> anyhow::Result<()> {
    let addr: SocketAddr = cfg.bind.parse().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("bad bind addr: {}", cfg.bind),
        )
    })?;
    let app = build_app(state, cfg);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("anvaya daemon listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("installed ctrl-c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("installed SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    tracing::info!("shutdown signal received, draining requests");
}
