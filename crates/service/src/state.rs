//! Shared application state for the service runtime.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use store::MetadataStore;

use crate::ServiceError;

/// Default port for the local service.
///
/// Uses a port in the dynamic/private range (49152-65535) to avoid
/// conflicts with well-known services and reduce false positives from
/// security scanners in corporate environments.
pub const DEFAULT_PORT: u16 = 52337;

/// Default bind address (localhost only per DR-5).
pub const DEFAULT_HOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// Configuration for the service runtime.
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub data_root: PathBuf,
    pub host: IpAddr,
    pub port: u16,
}

impl ServiceConfig {
    pub fn new(data_root: PathBuf) -> Self {
        Self {
            data_root,
            host: DEFAULT_HOST,
            port: DEFAULT_PORT,
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_root.join("metadata.db")
    }

    pub fn blob_path(&self) -> PathBuf {
        self.data_root.join("blobs")
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

/// Shared state accessible from all request handlers.
///
/// Wraps the `MetadataStore` in an `Arc<Mutex<_>>` so handlers running on
/// the tokio thread pool can access it safely. SQLite in WAL mode supports
/// concurrent reads from multiple threads, but rusqlite's `Connection` is
/// `!Sync`, so we serialize access through a mutex.
///
/// This is an intentional simplification for the first slice. For a
/// single-user local service the mutex is not a bottleneck. If query
/// volume grows, a connection pool (e.g. `r2d2` or `deadpool-sqlite`)
/// or per-request `spawn_blocking` with short-lived connections would
/// allow true read concurrency.
#[derive(Clone)]
pub struct SharedState {
    pub(crate) db: Arc<Mutex<MetadataStore>>,
    pub(crate) config: ServiceConfig,
    pub(crate) started_at: Instant,
}

impl SharedState {
    /// Opens the shared store and creates the application state.
    pub fn open(config: &ServiceConfig) -> Result<Self, ServiceError> {
        let db_path = config.db_path();

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ServiceError::Runtime(format!(
                    "cannot create data directory {}: {e}",
                    parent.display()
                ))
            })?;
        }

        let db = MetadataStore::open(&db_path)?;

        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            config: config.clone(),
            started_at: Instant::now(),
        })
    }

    /// Returns the number of registered repos.
    pub fn repo_count(&self) -> usize {
        self.db
            .lock()
            .ok()
            .and_then(|db| db.repos().list_ids().ok())
            .map_or(0, |ids| ids.len())
    }

    /// Acquires the database lock and runs `f`, flattening lock and store
    /// errors into [`ServiceError`].
    ///
    /// Closures should return `Result<T, E>` where `E: Into<ServiceError>`.
    /// This eliminates the double-`Result` nesting that would otherwise
    /// appear in every route handler.
    pub(crate) fn with_db<F, T, E>(&self, f: F) -> Result<T, ServiceError>
    where
        F: FnOnce(&MetadataStore) -> Result<T, E>,
        E: Into<ServiceError>,
    {
        let guard = self.db.lock().map_err(|_| ServiceError::LockPoisoned)?;
        f(&guard).map_err(Into::into)
    }

    /// Acquires the database lock mutably and runs `f`, flattening errors.
    #[allow(dead_code)] // Will be used by repo lifecycle endpoints in #153.
    pub(crate) fn with_db_mut<F, T, E>(&self, f: F) -> Result<T, ServiceError>
    where
        F: FnOnce(&mut MetadataStore) -> Result<T, E>,
        E: Into<ServiceError>,
    {
        let mut guard = self.db.lock().map_err(|_| ServiceError::LockPoisoned)?;
        f(&mut guard).map_err(Into::into)
    }
}
