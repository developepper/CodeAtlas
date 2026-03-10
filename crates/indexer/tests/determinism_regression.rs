//! Determinism and idempotency regression tests.
//!
//! Verifies that repeated indexing runs with the same inputs produce stable,
//! reproducible outputs. These tests guard against regressions in:
//!
//! - Symbol ID construction (same content → same IDs across re-indexes)
//! - Enrichment outputs (summaries, keywords)
//! - Pipeline idempotency (N runs → identical DB state)
//! - Incremental vs full reindex equivalence
//! - Ordering stability (file paths, symbol IDs, query results)
//!
//! See spec §16.2 (Determinism Requirements).

use adapter_api::{AdapterPolicy, AdapterRouter, LanguageAdapter};
use adapter_syntax_treesitter::{create_adapter, supported_languages, TreeSitterAdapter};
use indexer::{run, PipelineContext};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

struct TreeSitterRouter {
    adapters: Vec<TreeSitterAdapter>,
}

impl TreeSitterRouter {
    fn new() -> Self {
        let adapters = supported_languages()
            .iter()
            .filter_map(|lang| create_adapter(lang))
            .collect();
        Self { adapters }
    }
}

impl AdapterRouter for TreeSitterRouter {
    fn select(&self, language: &str, _policy: AdapterPolicy) -> Vec<&dyn LanguageAdapter> {
        self.adapters
            .iter()
            .filter(|a| a.language() == language)
            .map(|a| a as &dyn LanguageAdapter)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Fixture repo
// ---------------------------------------------------------------------------

/// Creates a non-trivial multi-file Rust repo for determinism testing.
/// Uses multiple symbol kinds to exercise enrichment thoroughly.
fn create_fixture_repo(dir: &std::path::Path) {
    let src = dir.join("src");
    std::fs::create_dir_all(&src).expect("create src dir");

    std::fs::write(
        src.join("lib.rs"),
        r#"/// Application configuration.
pub struct Config {
    pub name: String,
    pub debug: bool,
}

/// Maximum retries.
pub const MAX_RETRIES: u32 = 3;

impl Config {
    /// Creates a default config.
    pub fn default_config() -> Self {
        Self {
            name: "app".to_string(),
            debug: false,
        }
    }

    /// Returns the display name.
    pub fn display_name(&self) -> &str {
        &self.name
    }
}

/// Greeting helper.
pub fn greet(config: &Config) -> String {
    format!("Hello from {}!", config.name)
}
"#,
    )
    .expect("write lib.rs");

    std::fs::write(
        src.join("main.rs"),
        r#"mod lib;

fn main() {
    println!("starting");
}

/// Setup logging.
fn setup_logging() {
    // placeholder
}
"#,
    )
    .expect("write main.rs");

    std::fs::write(
        src.join("utils.rs"),
        r#"/// A simple cache type.
pub type Cache = std::collections::HashMap<String, String>;

/// Formats a duration in human-readable form.
pub fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m {}s", secs / 60, secs % 60)
    }
}

/// Parses a key-value pair from a string.
pub fn parse_kv(input: &str) -> Option<(&str, &str)> {
    input.split_once('=')
}
"#,
    )
    .expect("write utils.rs");
}

/// Snapshot of all determinism-relevant state from the metadata store.
#[derive(Debug, PartialEq)]
struct DbSnapshot {
    file_paths: Vec<String>,
    file_hashes: Vec<(String, String)>,
    file_summaries: Vec<(String, String)>,
    file_symbol_counts: Vec<(String, u64)>,
    symbol_ids: Vec<String>,
    symbol_summaries: Vec<(String, Option<String>)>,
    symbol_keywords: Vec<(String, Option<Vec<String>>)>,
    repo_file_count: u64,
    repo_symbol_count: u64,
    repo_language_counts: std::collections::BTreeMap<String, u64>,
}

fn snapshot_db(db: &store::MetadataStore, repo_id: &str) -> DbSnapshot {
    let file_paths = db.files().list_paths(repo_id).unwrap();

    let mut file_hashes = Vec::new();
    let mut file_summaries = Vec::new();
    let mut file_symbol_counts = Vec::new();
    for path in &file_paths {
        let f = db.files().get(repo_id, path).unwrap().unwrap();
        file_hashes.push((path.clone(), f.file_hash.clone()));
        file_summaries.push((path.clone(), f.summary.clone()));
        file_symbol_counts.push((path.clone(), f.symbol_count));
    }

    let mut symbol_ids = Vec::new();
    let mut symbol_summaries = Vec::new();
    let mut symbol_keywords = Vec::new();
    for path in &file_paths {
        let ids = db.symbols().list_ids_for_file(repo_id, path).unwrap();
        for id in &ids {
            let sym = db.symbols().get(id).unwrap().unwrap();
            symbol_summaries.push((id.clone(), sym.summary.clone()));
            symbol_keywords.push((id.clone(), sym.keywords.clone()));
        }
        symbol_ids.extend(ids);
    }

    let repo = db.repos().get(repo_id).unwrap().unwrap();

    DbSnapshot {
        file_paths,
        file_hashes,
        file_summaries,
        file_symbol_counts,
        symbol_ids,
        symbol_summaries,
        symbol_keywords,
        repo_file_count: repo.file_count,
        repo_symbol_count: repo.symbol_count,
        repo_language_counts: repo.language_counts,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Two independent pipeline runs against the same repo content produce
/// identical DB state: same symbol IDs, summaries, keywords, hashes, and
/// aggregates.
#[test]
fn full_index_produces_identical_state_across_independent_runs() {
    let repo_dir = TempDir::new().expect("create temp dir");
    create_fixture_repo(repo_dir.path());

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = store::BlobStore::open(&blob_dir.path().join("blobs")).unwrap();
    let router = TreeSitterRouter::new();

    // Run 1: fresh DB.
    let mut db1 = store::MetadataStore::open_in_memory().unwrap();
    let ctx = PipelineContext {
        repo_id: "det-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };
    run(&ctx, &mut db1, &blob_store).expect("run 1");

    // Run 2: separate fresh DB, same inputs.
    let mut db2 = store::MetadataStore::open_in_memory().unwrap();
    run(&ctx, &mut db2, &blob_store).expect("run 2");

    let snap1 = snapshot_db(&db1, "det-repo");
    let snap2 = snapshot_db(&db2, "det-repo");

    assert_eq!(
        snap1.file_paths, snap2.file_paths,
        "file paths should be identical"
    );
    assert_eq!(
        snap1.file_hashes, snap2.file_hashes,
        "file hashes should be identical"
    );
    assert_eq!(
        snap1.file_summaries, snap2.file_summaries,
        "file summaries should be identical"
    );
    assert_eq!(
        snap1.file_symbol_counts, snap2.file_symbol_counts,
        "file symbol counts should be identical"
    );
    assert_eq!(
        snap1.symbol_ids, snap2.symbol_ids,
        "symbol IDs should be identical"
    );
    assert_eq!(
        snap1.symbol_summaries, snap2.symbol_summaries,
        "symbol summaries should be identical"
    );
    assert_eq!(
        snap1.symbol_keywords, snap2.symbol_keywords,
        "symbol keywords should be identical"
    );
    assert_eq!(
        snap1.repo_file_count, snap2.repo_file_count,
        "repo file count should be identical"
    );
    assert_eq!(
        snap1.repo_symbol_count, snap2.repo_symbol_count,
        "repo symbol count should be identical"
    );
    assert_eq!(
        snap1.repo_language_counts, snap2.repo_language_counts,
        "repo language counts should be identical"
    );
}

/// Re-indexing the same content in-place (same DB) produces the same state.
/// This catches bugs where upsert logic or stale-cleanup perturbs existing
/// records.
#[test]
fn reindex_same_content_is_idempotent() {
    let repo_dir = TempDir::new().expect("create temp dir");
    create_fixture_repo(repo_dir.path());

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = store::BlobStore::open(&blob_dir.path().join("blobs")).unwrap();
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().unwrap();

    let ctx = PipelineContext {
        repo_id: "idem-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    // Run 1: full index.
    run(&ctx, &mut db, &blob_store).expect("run 1");
    let snap1 = snapshot_db(&db, "idem-repo");

    // Run 2: incremental (no changes, should be no-op).
    let r2 = run(&ctx, &mut db, &blob_store).expect("run 2");
    assert_eq!(r2.metrics.files_unchanged, 3);
    assert_eq!(r2.metrics.files_parsed, 0);
    let snap2 = snapshot_db(&db, "idem-repo");

    assert_eq!(
        snap1, snap2,
        "DB state should be identical after no-op re-index"
    );

    // Run 3: third run to verify continued stability.
    run(&ctx, &mut db, &blob_store).expect("run 3");
    let snap3 = snapshot_db(&db, "idem-repo");

    assert_eq!(snap1, snap3, "DB state should be identical after third run");
}

/// Symbol IDs remain stable across re-index when the symbol identity
/// (file path, qualified name, kind) has not changed, even when file
/// content changes around the symbol.
#[test]
fn symbol_ids_stable_when_identity_unchanged() {
    let repo_dir = TempDir::new().expect("create temp dir");
    std::fs::write(
        repo_dir.path().join("lib.rs"),
        "pub fn alpha() {}\npub fn beta() {}\n",
    )
    .expect("write lib.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = store::BlobStore::open(&blob_dir.path().join("blobs")).unwrap();
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().unwrap();

    let ctx = PipelineContext {
        repo_id: "id-stable-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    // Run 1.
    run(&ctx, &mut db, &blob_store).expect("run 1");
    let ids1 = db
        .symbols()
        .list_ids_for_file("id-stable-repo", "lib.rs")
        .unwrap();
    assert!(ids1.len() >= 2);

    // Modify file: add a comment and a new function, keeping alpha and beta.
    std::fs::write(
        repo_dir.path().join("lib.rs"),
        "// updated\npub fn alpha() {}\npub fn beta() {}\npub fn gamma() {}\n",
    )
    .expect("rewrite lib.rs");

    // Run 2.
    run(&ctx, &mut db, &blob_store).expect("run 2");
    let ids2 = db
        .symbols()
        .list_ids_for_file("id-stable-repo", "lib.rs")
        .unwrap();

    // alpha and beta IDs should be unchanged.
    for old_id in &ids1 {
        assert!(
            ids2.contains(old_id),
            "symbol ID '{}' should survive re-index with same identity",
            old_id
        );
    }

    // gamma should be new.
    assert!(ids2.len() > ids1.len(), "new symbol gamma should appear");
}

/// After modifying a file and re-indexing, the file hash updates but all
/// other files' hashes remain stable.
#[test]
fn file_hashes_stable_for_unchanged_files() {
    let repo_dir = TempDir::new().expect("create temp dir");
    create_fixture_repo(repo_dir.path());

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = store::BlobStore::open(&blob_dir.path().join("blobs")).unwrap();
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().unwrap();

    let ctx = PipelineContext {
        repo_id: "hash-stable-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    run(&ctx, &mut db, &blob_store).expect("run 1");
    let hashes1 = db.files().list_hash_map("hash-stable-repo").unwrap();

    // Modify only utils.rs.
    std::fs::write(
        repo_dir.path().join("src/utils.rs"),
        "pub fn new_util() {}\n",
    )
    .expect("rewrite utils.rs");

    run(&ctx, &mut db, &blob_store).expect("run 2");
    let hashes2 = db.files().list_hash_map("hash-stable-repo").unwrap();

    // lib.rs and main.rs hashes unchanged.
    assert_eq!(
        hashes1.get("src/lib.rs"),
        hashes2.get("src/lib.rs"),
        "lib.rs hash should be stable"
    );
    assert_eq!(
        hashes1.get("src/main.rs"),
        hashes2.get("src/main.rs"),
        "main.rs hash should be stable"
    );

    // utils.rs hash should have changed.
    assert_ne!(
        hashes1.get("src/utils.rs"),
        hashes2.get("src/utils.rs"),
        "utils.rs hash should change after modification"
    );
}

/// Incremental index (after initial full index) produces the same final
/// state as a fresh full index against the modified repo.
#[test]
fn incremental_and_full_reindex_produce_equivalent_state() {
    let repo_dir = TempDir::new().expect("create temp dir");
    create_fixture_repo(repo_dir.path());

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = store::BlobStore::open(&blob_dir.path().join("blobs")).unwrap();
    let router = TreeSitterRouter::new();

    // Path A: full index → modify → incremental re-index.
    let mut db_incremental = store::MetadataStore::open_in_memory().unwrap();
    let ctx = PipelineContext {
        repo_id: "equiv-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };
    run(&ctx, &mut db_incremental, &blob_store).expect("incremental: run 1");

    // Modify utils.rs, delete main.rs.
    std::fs::write(
        repo_dir.path().join("src/utils.rs"),
        "pub fn updated_util() {}\n",
    )
    .expect("rewrite utils.rs");
    std::fs::remove_file(repo_dir.path().join("src/main.rs")).expect("remove main.rs");

    run(&ctx, &mut db_incremental, &blob_store).expect("incremental: run 2");
    let snap_incremental = snapshot_db(&db_incremental, "equiv-repo");

    // Path B: fresh full index against the modified repo.
    let mut db_fresh = store::MetadataStore::open_in_memory().unwrap();
    run(&ctx, &mut db_fresh, &blob_store).expect("fresh: run 1");
    let snap_fresh = snapshot_db(&db_fresh, "equiv-repo");

    // The two paths should produce identical state.
    assert_eq!(
        snap_incremental.file_paths, snap_fresh.file_paths,
        "file paths should match"
    );
    assert_eq!(
        snap_incremental.file_hashes, snap_fresh.file_hashes,
        "file hashes should match"
    );
    assert_eq!(
        snap_incremental.symbol_ids, snap_fresh.symbol_ids,
        "symbol IDs should match"
    );
    assert_eq!(
        snap_incremental.symbol_summaries, snap_fresh.symbol_summaries,
        "symbol summaries should match"
    );
    assert_eq!(
        snap_incremental.symbol_keywords, snap_fresh.symbol_keywords,
        "symbol keywords should match"
    );
    assert_eq!(
        snap_incremental.repo_file_count, snap_fresh.repo_file_count,
        "repo file count should match"
    );
    assert_eq!(
        snap_incremental.repo_symbol_count, snap_fresh.repo_symbol_count,
        "repo symbol count should match"
    );
    assert_eq!(
        snap_incremental.repo_language_counts, snap_fresh.repo_language_counts,
        "repo language counts should match"
    );
}

/// File paths and symbol IDs returned from the store are always in sorted
/// order, regardless of insertion order.
#[test]
fn store_ordering_is_deterministic() {
    let repo_dir = TempDir::new().expect("create temp dir");
    create_fixture_repo(repo_dir.path());

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = store::BlobStore::open(&blob_dir.path().join("blobs")).unwrap();
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().unwrap();

    let ctx = PipelineContext {
        repo_id: "order-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    run(&ctx, &mut db, &blob_store).expect("run");

    // File paths should be sorted.
    let paths = db.files().list_paths("order-repo").unwrap();
    let mut sorted_paths = paths.clone();
    sorted_paths.sort();
    assert_eq!(paths, sorted_paths, "file paths should be sorted");

    // Symbol IDs within each file should be sorted.
    for path in &paths {
        let ids = db.symbols().list_ids_for_file("order-repo", path).unwrap();
        let mut sorted_ids = ids.clone();
        sorted_ids.sort();
        assert_eq!(
            ids, sorted_ids,
            "symbol IDs for '{}' should be sorted",
            path
        );
    }
}

/// Tie-breaking in symbol search results uses symbol ID as secondary sort
/// key, producing stable ordering across repeated queries and re-indexes.
#[test]
fn search_tie_ordering_stable_across_reindex() {
    use query_engine::{QueryFilters, QueryService, StoreQueryService, SymbolQuery};

    let repo_dir = TempDir::new().expect("create temp dir");
    // Create multiple functions with similar names to provoke tie scores.
    std::fs::write(
        repo_dir.path().join("lib.rs"),
        r#"pub fn run_alpha() {}
pub fn run_beta() {}
pub fn run_gamma() {}
pub fn run_delta() {}
"#,
    )
    .expect("write lib.rs");

    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = store::BlobStore::open(&blob_dir.path().join("blobs")).unwrap();
    let router = TreeSitterRouter::new();
    let mut db = store::MetadataStore::open_in_memory().unwrap();

    let ctx = PipelineContext {
        repo_id: "tie-repo".to_string(),
        source_root: repo_dir.path().to_path_buf(),
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
    };

    // Run 1.
    run(&ctx, &mut db, &blob_store).expect("run 1");
    let svc = StoreQueryService::new(&db);
    let query = SymbolQuery {
        repo_id: "tie-repo".into(),
        text: "run".into(),
        filters: QueryFilters::default(),
        limit: 10,
        offset: 0,
    };
    let r1 = svc.search_symbols(&query).unwrap();
    let ids1: Vec<&str> = r1.items.iter().map(|s| s.record.id.as_str()).collect();

    // Modify file to force re-index but keep same symbols.
    std::fs::write(
        repo_dir.path().join("lib.rs"),
        r#"// comment added
pub fn run_alpha() {}
pub fn run_beta() {}
pub fn run_gamma() {}
pub fn run_delta() {}
"#,
    )
    .expect("rewrite lib.rs");

    // Run 2.
    run(&ctx, &mut db, &blob_store).expect("run 2");
    let svc = StoreQueryService::new(&db);
    let r2 = svc.search_symbols(&query).unwrap();
    let ids2: Vec<&str> = r2.items.iter().map(|s| s.record.id.as_str()).collect();

    assert_eq!(
        ids1, ids2,
        "search result ordering should be stable across re-index"
    );

    // Scores should also be identical.
    let scores1: Vec<f32> = r1.items.iter().map(|s| s.score).collect();
    let scores2: Vec<f32> = r2.items.iter().map(|s| s.score).collect();
    assert_eq!(
        scores1, scores2,
        "scores should be identical for same symbols"
    );
}

/// Multiple full lifecycle runs (add → modify → delete → stabilize) produce
/// stable final state when starting from identical initial conditions.
/// Each invocation uses its own temp directory so the filesystem is clean
/// at the start of every run.
#[test]
fn lifecycle_determinism_across_independent_runs() {
    let blob_dir = TempDir::new().expect("blob temp dir");
    let blob_store = store::BlobStore::open(&blob_dir.path().join("blobs")).unwrap();
    let router = TreeSitterRouter::new();

    let run_lifecycle = |db: &mut store::MetadataStore| {
        let repo_dir = TempDir::new().expect("create temp dir");
        let ctx = PipelineContext {
            repo_id: "lc-repo".to_string(),
            source_root: repo_dir.path().to_path_buf(),
            router: &router,
            default_policy: AdapterPolicy::SyntaxOnly,
            correlation_id: None,
        };

        // Step 1: Initial files.
        std::fs::write(repo_dir.path().join("a.rs"), "pub fn a() {}\n").unwrap();
        std::fs::write(repo_dir.path().join("b.rs"), "pub fn b() {}\n").unwrap();
        run(&ctx, db, &blob_store).unwrap();

        // Step 2: Modify a.rs, add c.rs.
        std::fs::write(repo_dir.path().join("a.rs"), "pub fn a_v2() {}\n").unwrap();
        std::fs::write(repo_dir.path().join("c.rs"), "pub fn c() {}\n").unwrap();
        run(&ctx, db, &blob_store).unwrap();

        // Step 3: Delete b.rs.
        std::fs::remove_file(repo_dir.path().join("b.rs")).unwrap();
        run(&ctx, db, &blob_store).unwrap();

        snapshot_db(db, "lc-repo")
    };

    let mut db1 = store::MetadataStore::open_in_memory().unwrap();
    let snap1 = run_lifecycle(&mut db1);

    let mut db2 = store::MetadataStore::open_in_memory().unwrap();
    let snap2 = run_lifecycle(&mut db2);

    assert_eq!(snap1, snap2, "lifecycle should produce identical state");
}
