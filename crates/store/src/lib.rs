//! Persistence layer for CodeAtlas.
//!
//! Provides [`MetadataStore`] (SQLite) for persisting repository, file,
//! and symbol records, and [`BlobStore`] (filesystem) for content-addressed
//! blob storage of raw file snapshots.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

pub mod blob_store;
mod file_store;
mod migrations;
mod repo_store;
mod symbol_store;

pub use blob_store::{content_hash, BlobStore};
pub use file_store::FileStore;
pub use migrations::{rollback_to, SCHEMA_VERSION};
pub use repo_store::RepoStore;
pub use symbol_store::SymbolStore;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by the metadata or blob store.
#[derive(Debug)]
pub enum StoreError {
    /// A SQLite operation failed.
    Sqlite(rusqlite::Error),
    /// A schema migration failed.
    Migration { version: u32, reason: String },
    /// A record failed validation before persistence.
    Validation(String),
    /// A blob storage operation failed.
    Blob {
        path: Option<PathBuf>,
        reason: String,
    },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "sqlite error: {e}"),
            Self::Migration { version, reason } => {
                write!(f, "migration v{version} failed: {reason}")
            }
            Self::Validation(msg) => write!(f, "validation error: {msg}"),
            Self::Blob { path, reason } => {
                if let Some(path) = path {
                    write!(f, "blob error at '{}': {reason}", path.display())
                } else {
                    write!(f, "blob error: {reason}")
                }
            }
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlite(e) => Some(e),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

// ---------------------------------------------------------------------------
// MetadataStore
// ---------------------------------------------------------------------------

/// SQLite-backed metadata store for repos, files, and symbols.
///
/// Owns a single [`Connection`] and applies migrations on open.
pub struct MetadataStore {
    conn: Connection,
}

impl MetadataStore {
    /// Opens (or creates) a SQLite database at the given path and applies
    /// all pending migrations.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// Opens an in-memory SQLite database. Useful for tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, StoreError> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        migrations::apply_all(&conn)?;
        Ok(Self { conn })
    }

    /// Returns a reference to the underlying connection (for advanced use).
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Returns the repo store accessor.
    #[must_use]
    pub fn repos(&self) -> RepoStore<'_> {
        RepoStore::new(&self.conn)
    }

    /// Returns the file store accessor.
    #[must_use]
    pub fn files(&self) -> FileStore<'_> {
        FileStore::new(&self.conn)
    }

    /// Returns the symbol store accessor.
    #[must_use]
    pub fn symbols(&self) -> SymbolStore<'_> {
        SymbolStore::new(&self.conn)
    }

    /// Returns the current schema version stored in the database.
    pub fn schema_version(&self) -> Result<u32, StoreError> {
        migrations::current_version(&self.conn)
    }
}
