//! Versioned schema migrations.
//!
//! Each migration is a numbered SQL script. The `schema_meta` table tracks
//! which migrations have been applied. Migrations run in a transaction and
//! are idempotent (re-running a previously applied migration is a no-op).

use rusqlite::Connection;

use crate::StoreError;

/// Current schema version (latest migration number).
pub const SCHEMA_VERSION: u32 = 4;

/// All migrations in order. Each entry is `(version, up_sql, down_sql)`.
const MIGRATIONS: &[(u32, &str, &str)] = &[
    (1, V1_UP, V1_DOWN),
    (2, V2_UP, V2_DOWN),
    (3, V3_UP, V3_DOWN),
    (4, V4_UP, V4_DOWN),
];

// ---------------------------------------------------------------------------
// V1: baseline schema
// ---------------------------------------------------------------------------

const V1_UP: &str = r#"
CREATE TABLE IF NOT EXISTS repos (
    repo_id         TEXT PRIMARY KEY NOT NULL,
    display_name    TEXT NOT NULL,
    source_root     TEXT NOT NULL,
    indexed_at      TEXT NOT NULL,
    index_version   TEXT NOT NULL,
    language_counts TEXT NOT NULL DEFAULT '{}',
    file_count      INTEGER NOT NULL DEFAULT 0,
    symbol_count    INTEGER NOT NULL DEFAULT 0,
    git_head        TEXT
);

CREATE TABLE IF NOT EXISTS files (
    repo_id         TEXT NOT NULL,
    file_path       TEXT NOT NULL,
    language        TEXT NOT NULL,
    file_hash       TEXT NOT NULL,
    summary         TEXT NOT NULL DEFAULT '',
    symbol_count    INTEGER NOT NULL DEFAULT 0,
    semantic_pct    REAL NOT NULL DEFAULT 0.0,
    syntax_pct      REAL NOT NULL DEFAULT 0.0,
    updated_at      TEXT NOT NULL,
    PRIMARY KEY (repo_id, file_path),
    FOREIGN KEY (repo_id) REFERENCES repos(repo_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS symbols (
    id                      TEXT PRIMARY KEY NOT NULL,
    repo_id                 TEXT NOT NULL,
    file_path               TEXT NOT NULL,
    language                TEXT NOT NULL,
    kind                    TEXT NOT NULL,
    name                    TEXT NOT NULL,
    qualified_name          TEXT NOT NULL,
    signature               TEXT NOT NULL,
    start_line              INTEGER NOT NULL,
    end_line                INTEGER NOT NULL,
    start_byte              INTEGER NOT NULL,
    byte_length             INTEGER NOT NULL,
    content_hash            TEXT NOT NULL,
    quality_level           TEXT NOT NULL,
    confidence_score        REAL NOT NULL,
    source_adapter          TEXT NOT NULL,
    indexed_at              TEXT NOT NULL,
    docstring               TEXT,
    summary                 TEXT,
    parent_symbol_id        TEXT,
    keywords                TEXT,
    decorators_or_attributes TEXT,
    semantic_refs           TEXT,
    FOREIGN KEY (repo_id, file_path) REFERENCES files(repo_id, file_path) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_symbols_repo_file ON symbols(repo_id, file_path);
CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
CREATE INDEX IF NOT EXISTS idx_symbols_qualified_name ON symbols(qualified_name);
CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind);
CREATE INDEX IF NOT EXISTS idx_files_repo ON files(repo_id);
"#;

const V1_DOWN: &str = r#"
DROP INDEX IF EXISTS idx_files_repo;
DROP INDEX IF EXISTS idx_symbols_kind;
DROP INDEX IF EXISTS idx_symbols_qualified_name;
DROP INDEX IF EXISTS idx_symbols_name;
DROP INDEX IF EXISTS idx_symbols_repo_file;
DROP TABLE IF EXISTS symbols;
DROP TABLE IF EXISTS files;
DROP TABLE IF EXISTS repos;
"#;

// ---------------------------------------------------------------------------
// V2: FTS5 full-text search index on symbols
// ---------------------------------------------------------------------------

const V2_UP: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS symbols_fts USING fts5(
    id UNINDEXED,
    name,
    qualified_name,
    signature,
    docstring,
    summary,
    keywords,
    content='symbols',
    content_rowid='rowid'
);

-- Populate from existing data.
INSERT INTO symbols_fts(symbols_fts) VALUES('rebuild');

-- Keep FTS in sync on insert/update/delete.
CREATE TRIGGER IF NOT EXISTS symbols_ai AFTER INSERT ON symbols BEGIN
    INSERT INTO symbols_fts(rowid, id, name, qualified_name, signature, docstring, summary, keywords)
    VALUES (new.rowid, new.id, new.name, new.qualified_name, new.signature, new.docstring, new.summary, new.keywords);
END;

CREATE TRIGGER IF NOT EXISTS symbols_ad AFTER DELETE ON symbols BEGIN
    INSERT INTO symbols_fts(symbols_fts, rowid, id, name, qualified_name, signature, docstring, summary, keywords)
    VALUES ('delete', old.rowid, old.id, old.name, old.qualified_name, old.signature, old.docstring, old.summary, old.keywords);
END;

CREATE TRIGGER IF NOT EXISTS symbols_au AFTER UPDATE ON symbols BEGIN
    INSERT INTO symbols_fts(symbols_fts, rowid, id, name, qualified_name, signature, docstring, summary, keywords)
    VALUES ('delete', old.rowid, old.id, old.name, old.qualified_name, old.signature, old.docstring, old.summary, old.keywords);
    INSERT INTO symbols_fts(rowid, id, name, qualified_name, signature, docstring, summary, keywords)
    VALUES (new.rowid, new.id, new.name, new.qualified_name, new.signature, new.docstring, new.summary, new.keywords);
END;
"#;

const V2_DOWN: &str = r#"
DROP TRIGGER IF EXISTS symbols_au;
DROP TRIGGER IF EXISTS symbols_ad;
DROP TRIGGER IF EXISTS symbols_ai;
DROP TABLE IF EXISTS symbols_fts;
"#;

// ---------------------------------------------------------------------------
// V3: repo catalog metadata for persistent service model
// ---------------------------------------------------------------------------

const V3_UP: &str = r#"
-- Repo catalog metadata for persistent service model.
ALTER TABLE repos ADD COLUMN registered_at TEXT;
ALTER TABLE repos ADD COLUMN indexing_status TEXT NOT NULL DEFAULT 'ready';
ALTER TABLE repos ADD COLUMN freshness_status TEXT NOT NULL DEFAULT 'fresh';

-- Migrate existing symbol IDs to the repo-prefixed format
-- ({repo_id}//{old_id}) so all symbols have globally unique IDs
-- in a shared store.
UPDATE symbols SET id = repo_id || '//' || id
    WHERE id NOT LIKE '%//%';

-- FTS content is synced via triggers on INSERT/UPDATE/DELETE, but a
-- bulk UPDATE bypasses content-sync triggers for the *old* row shape
-- (the trigger fires with the new values). Rebuild to ensure the FTS
-- index reflects the updated IDs.
INSERT INTO symbols_fts(symbols_fts) VALUES('rebuild');
"#;

const V3_DOWN: &str = r#"
-- Strip repo_id prefix from symbol IDs to restore the old format.
UPDATE symbols SET id = SUBSTR(id, INSTR(id, '//') + 2)
    WHERE id LIKE '%//%';
INSERT INTO symbols_fts(symbols_fts) VALUES('rebuild');

ALTER TABLE repos DROP COLUMN freshness_status;
ALTER TABLE repos DROP COLUMN indexing_status;
ALTER TABLE repos DROP COLUMN registered_at;
"#;

// ---------------------------------------------------------------------------
// V4: capability tier model (Epic 17, Ticket 175)
// ---------------------------------------------------------------------------

const V4_UP: &str = r#"
-- Rename quality_level -> capability_tier on symbols and migrate values.
-- Semantic symbols become semantic_only (not syntax_plus_semantic) because
-- we cannot retroactively determine whether a syntax baseline existed.
-- The file-level tier is derived from its symbols below.
ALTER TABLE symbols RENAME COLUMN quality_level TO capability_tier;
UPDATE symbols SET capability_tier = 'semantic_only' WHERE capability_tier = 'semantic';
UPDATE symbols SET capability_tier = 'syntax_only' WHERE capability_tier = 'syntax';

-- Rename source_adapter -> source_backend on symbols.
ALTER TABLE symbols RENAME COLUMN source_adapter TO source_backend;

-- Add capability_tier to files, replacing semantic_pct/syntax_pct.
-- Derive file tier from the symbols it contains:
--   - Has both syntax_only and semantic_only symbols → syntax_plus_semantic
--   - Has only semantic_only symbols (and semantic_pct > 0) → semantic_only
--   - Has only syntax_only symbols → syntax_only
--   - No symbols → file_only
ALTER TABLE files ADD COLUMN capability_tier TEXT NOT NULL DEFAULT 'file_only';

-- Files with both syntax and semantic symbols.
UPDATE files SET capability_tier = 'syntax_plus_semantic'
    WHERE symbol_count > 0
    AND semantic_pct > 0.0
    AND syntax_pct > 0.0;

-- Files with only semantic symbols (no syntax contribution).
UPDATE files SET capability_tier = 'semantic_only'
    WHERE capability_tier = 'file_only'
    AND symbol_count > 0
    AND semantic_pct > 0.0
    AND syntax_pct = 0.0;

-- Files with only syntax symbols.
UPDATE files SET capability_tier = 'syntax_only'
    WHERE capability_tier = 'file_only'
    AND symbol_count > 0;

ALTER TABLE files DROP COLUMN semantic_pct;
ALTER TABLE files DROP COLUMN syntax_pct;

-- Structural fields for broad syntax indexing (Epic 17).
ALTER TABLE symbols ADD COLUMN container_symbol_id TEXT;
ALTER TABLE symbols ADD COLUMN namespace_path TEXT;
ALTER TABLE symbols ADD COLUMN raw_kind TEXT;
ALTER TABLE symbols ADD COLUMN modifiers TEXT;

-- Rebuild FTS to reflect renamed columns.
INSERT INTO symbols_fts(symbols_fts) VALUES('rebuild');
"#;

const V4_DOWN: &str = r#"
-- Drop structural fields.
ALTER TABLE symbols DROP COLUMN modifiers;
ALTER TABLE symbols DROP COLUMN raw_kind;
ALTER TABLE symbols DROP COLUMN namespace_path;
ALTER TABLE symbols DROP COLUMN container_symbol_id;

-- Restore source_adapter name.
ALTER TABLE symbols RENAME COLUMN source_backend TO source_adapter;

-- Restore quality_level name and values.
ALTER TABLE symbols RENAME COLUMN capability_tier TO quality_level;
UPDATE symbols SET quality_level = 'semantic' WHERE quality_level IN ('syntax_plus_semantic', 'semantic_only');
UPDATE symbols SET quality_level = 'syntax' WHERE quality_level IN ('syntax_only', 'file_only');

-- Restore semantic_pct/syntax_pct on files.
ALTER TABLE files ADD COLUMN semantic_pct REAL NOT NULL DEFAULT 0.0;
ALTER TABLE files ADD COLUMN syntax_pct REAL NOT NULL DEFAULT 0.0;
UPDATE files SET semantic_pct = 100.0 WHERE capability_tier IN ('syntax_plus_semantic', 'semantic_only');
UPDATE files SET syntax_pct = 100.0 WHERE capability_tier IN ('syntax_only', 'syntax_plus_semantic');

ALTER TABLE files DROP COLUMN capability_tier;

INSERT INTO symbols_fts(symbols_fts) VALUES('rebuild');
"#;

// ---------------------------------------------------------------------------
// Migration engine
// ---------------------------------------------------------------------------

/// Applies all pending migrations.
pub fn apply_all(conn: &Connection) -> Result<(), StoreError> {
    ensure_meta_table(conn)?;
    let current = current_version(conn)?;

    for &(version, up_sql, _) in MIGRATIONS {
        if version > current {
            conn.execute_batch(up_sql)
                .map_err(|e| StoreError::Migration {
                    version,
                    reason: e.to_string(),
                })?;
            set_version(conn, version)?;
        }
    }
    Ok(())
}

/// Rolls back to a target version (inclusive). Migrations above `target`
/// are reverted in reverse order.
pub fn rollback_to(conn: &Connection, target: u32) -> Result<(), StoreError> {
    ensure_meta_table(conn)?;
    let current = current_version(conn)?;

    for &(version, _, down_sql) in MIGRATIONS.iter().rev() {
        if version > target && version <= current {
            conn.execute_batch(down_sql)
                .map_err(|e| StoreError::Migration {
                    version,
                    reason: e.to_string(),
                })?;
            set_version(conn, version.saturating_sub(1))?;
        }
    }
    Ok(())
}

fn ensure_meta_table(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_meta (
            key   TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// Returns the current schema version (0 if no migrations applied).
pub fn current_version(conn: &Connection) -> Result<u32, StoreError> {
    ensure_meta_table(conn)?;
    let mut stmt = conn.prepare("SELECT value FROM schema_meta WHERE key = 'schema_version'")?;
    let version: Option<String> = stmt.query_row([], |row| row.get(0)).ok();
    match version {
        Some(v) => v.parse::<u32>().map_err(|_| StoreError::Migration {
            version: 0,
            reason: format!("invalid schema_version value: {v}"),
        }),
        None => Ok(0),
    }
}

fn set_version(conn: &Connection, version: u32) -> Result<(), StoreError> {
    conn.execute(
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('schema_version', ?1)",
        [version.to_string()],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn
    }

    #[test]
    fn apply_all_creates_tables() {
        let conn = memory_conn();
        apply_all(&conn).expect("apply migrations");

        // Verify tables exist by querying sqlite_master.
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"repos".to_string()));
        assert!(tables.contains(&"files".to_string()));
        assert!(tables.contains(&"symbols".to_string()));
        assert!(tables.contains(&"schema_meta".to_string()));
    }

    #[test]
    fn apply_all_is_idempotent() {
        let conn = memory_conn();
        apply_all(&conn).expect("first apply");
        apply_all(&conn).expect("second apply must succeed");
        assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn current_version_returns_latest_after_apply() {
        let conn = memory_conn();
        apply_all(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn current_version_returns_zero_before_apply() {
        let conn = memory_conn();
        assert_eq!(current_version(&conn).unwrap(), 0);
    }

    #[test]
    fn rollback_to_zero_drops_tables() {
        let conn = memory_conn();
        apply_all(&conn).unwrap();
        rollback_to(&conn, 0).unwrap();

        assert_eq!(current_version(&conn).unwrap(), 0);

        // Tables should be gone.
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('repos','files','symbols')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn rollback_then_reapply() {
        let conn = memory_conn();
        apply_all(&conn).unwrap();
        rollback_to(&conn, 0).unwrap();
        apply_all(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn v3_migration_rewrites_legacy_symbol_ids() {
        let conn = memory_conn();

        // Apply V1 + V2 only.
        for &(version, up_sql, _) in MIGRATIONS.iter().take(2) {
            ensure_meta_table(&conn).unwrap();
            conn.execute_batch(up_sql).unwrap();
            set_version(&conn, version).unwrap();
        }
        assert_eq!(current_version(&conn).unwrap(), 2);

        // Insert a repo, file, and symbol with old-format ID.
        conn.execute_batch(
            r#"
            INSERT INTO repos (repo_id, display_name, source_root, indexed_at, index_version)
                VALUES ('my-app', 'My App', '/repos/my-app', '2025-01-01T00:00:00Z', '1.0.0');
            INSERT INTO files (repo_id, file_path, language, file_hash, updated_at)
                VALUES ('my-app', 'src/lib.rs', 'rust', 'abc', '2025-01-01T00:00:00Z');
            INSERT INTO symbols (id, repo_id, file_path, language, kind, name, qualified_name,
                    signature, start_line, end_line, start_byte, byte_length,
                    content_hash, quality_level, confidence_score, source_adapter, indexed_at)
                VALUES ('src/lib.rs::Config#class', 'my-app', 'src/lib.rs', 'rust', 'class',
                    'Config', 'Config', 'struct Config', 1, 10, 0, 100,
                    'sha256:abc', 'syntax', 0.7, 'treesitter', '2025-01-01T00:00:00Z');
            "#,
        )
        .unwrap();

        // Verify old-format ID exists.
        let old_id: String = conn
            .query_row("SELECT id FROM symbols", [], |r| r.get(0))
            .unwrap();
        assert_eq!(old_id, "src/lib.rs::Config#class");

        // Apply V3 and V4 (apply_all runs all pending).
        apply_all(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);

        // Symbol ID is now repo-prefixed (V3) and quality_level → capability_tier (V4).
        let new_id: String = conn
            .query_row("SELECT id FROM symbols", [], |r| r.get(0))
            .unwrap();
        assert_eq!(new_id, "my-app//src/lib.rs::Config#class");

        // V4: quality_level renamed to capability_tier and value migrated.
        let tier: String = conn
            .query_row("SELECT capability_tier FROM symbols", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tier, "syntax_only");

        // V4: source_adapter renamed to source_backend.
        let backend: String = conn
            .query_row("SELECT source_backend FROM symbols", [], |r| r.get(0))
            .unwrap();
        assert_eq!(backend, "treesitter");

        // Rollback V3 strips the prefix (rollback V4 first, then V3).
        rollback_to(&conn, 2).unwrap();
        let reverted_id: String = conn
            .query_row("SELECT id FROM symbols", [], |r| r.get(0))
            .unwrap();
        assert_eq!(reverted_id, "src/lib.rs::Config#class");
    }
}
