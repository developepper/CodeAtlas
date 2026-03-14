//! Shared storage root resolution for the CodeAtlas CLI.
//!
//! The canonical default storage location is `~/.codeatlas/`. All CLI
//! commands that need a database path use this as the default when
//! `--db` is not specified.

use std::path::PathBuf;

/// Returns the default shared storage root, respecting the
/// `CODEATLAS_DATA_ROOT` override if set.
///
/// Default: `$HOME/.codeatlas` (Unix/macOS) or `%USERPROFILE%\.codeatlas`
/// (Windows).
pub fn default_data_root() -> Result<PathBuf, String> {
    if let Ok(root) = std::env::var("CODEATLAS_DATA_ROOT") {
        return Ok(PathBuf::from(root));
    }

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| {
            "cannot determine home directory; set CODEATLAS_DATA_ROOT or HOME".to_string()
        })?;

    Ok(PathBuf::from(home).join(".codeatlas"))
}

/// Returns the default metadata database path within the shared storage root.
pub fn default_db_path() -> Result<PathBuf, String> {
    Ok(default_data_root()?.join("metadata.db"))
}

/// Returns the default blob storage path within the shared storage root.
pub fn default_blob_path() -> Result<PathBuf, String> {
    Ok(default_data_root()?.join("blobs"))
}

/// Resolves the `(db_path, blob_path)` pair from an optional `--db`
/// override. When `db_override` is `None`, uses the shared storage root.
pub fn resolve_db_and_blob_paths(
    db_override: Option<PathBuf>,
) -> Result<(PathBuf, PathBuf), String> {
    if let Some(p) = db_override {
        let blobs = p
            .parent()
            .map_or_else(|| PathBuf::from("blobs"), |parent| parent.join("blobs"));
        Ok((p, blobs))
    } else {
        let root = default_data_root()?;
        Ok((root.join("metadata.db"), root.join("blobs")))
    }
}
