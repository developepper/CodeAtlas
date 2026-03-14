//! CodeAtlas persistent local HTTP service runtime.
//!
//! Provides a long-running HTTP server that owns a shared storage root and
//! exposes query, repo-catalog, and health/status APIs over localhost.
//!
//! The async runtime (`tokio` + `axum`) is scoped to this crate. Core
//! crates (`store`, `query-engine`, `server-mcp`) remain synchronous.

mod routes;
mod state;

pub use state::{ServiceConfig, DEFAULT_HOST, DEFAULT_PORT};

use std::net::SocketAddr;

use axum::Router;
use tracing::info;

/// Runs the persistent local service until shutdown is signalled.
///
/// Opens the shared store at the configured data root, builds the HTTP
/// router, and listens on the configured address. Returns when the server
/// shuts down (via SIGINT/SIGTERM).
pub async fn run_service(config: ServiceConfig) -> Result<(), ServiceError> {
    let shared = state::SharedState::open(&config)?;

    let bind_addr = config.bind_addr();

    let app = Router::new()
        .merge(routes::health_routes())
        .merge(routes::status_routes())
        .merge(routes::repo_routes())
        .merge(routes::query_routes())
        .with_state(shared.clone());

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(|e| ServiceError::Bind {
            addr: bind_addr,
            reason: e.to_string(),
        })?;

    let local_addr = listener.local_addr().map_err(|e| ServiceError::Bind {
        addr: bind_addr,
        reason: e.to_string(),
    })?;

    info!(
        addr = %local_addr,
        data_root = %config.data_root.display(),
        repos = shared.repo_count(),
        "codeatlas service started"
    );

    eprintln!(
        "codeatlas: service listening on http://{local_addr} (data-root: {})",
        config.data_root.display()
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| ServiceError::Runtime(e.to_string()))?;

    info!("codeatlas service stopped");
    eprintln!("codeatlas: service stopped");

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
    }
    eprintln!("\ncodeatlas: shutdown signal received");
}

// ── Error ──────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ServiceError {
    Store(store::StoreError),
    Query(query_engine::QueryError),
    LockPoisoned,
    Bind { addr: SocketAddr, reason: String },
    Runtime(String),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(e) => write!(f, "store error: {e}"),
            Self::Query(e) => write!(f, "query error: {e}"),
            Self::LockPoisoned => write!(f, "database lock poisoned"),
            Self::Bind { addr, reason } => {
                write!(f, "failed to bind to {addr}: {reason}")
            }
            Self::Runtime(e) => write!(f, "runtime error: {e}"),
        }
    }
}

impl std::error::Error for ServiceError {}

impl From<store::StoreError> for ServiceError {
    fn from(e: store::StoreError) -> Self {
        Self::Store(e)
    }
}

impl From<query_engine::QueryError> for ServiceError {
    fn from(e: query_engine::QueryError) -> Self {
        Self::Query(e)
    }
}
