//! `codeatlas repo` command family — repo catalog and lifecycle operations.

use std::path::PathBuf;

use indexer::PipelineContext;

use crate::error::CliError;
use crate::router;

/// Entry point for the `codeatlas repo` command family.
pub fn run(args: &[String]) -> Result<(), CliError> {
    let subcommand = args.first().map(|s| s.as_str());
    match subcommand {
        Some("add") => run_add(&args[1..]),
        Some("list") => run_list(&args[1..]),
        Some("status") => run_status(&args[1..]),
        Some("refresh") => run_refresh(&args[1..]),
        Some("remove") => run_remove(&args[1..]),
        Some("--help" | "-h") => {
            print_repo_help();
            Ok(())
        }
        Some(other) => Err(CliError::Usage(format!(
            "unknown repo subcommand: '{other}'\n\n{}",
            REPO_HELP
        ))),
        None => {
            print_repo_help();
            Err(CliError::Usage("missing repo subcommand".into()))
        }
    }
}

// ── repo add ──────────────────────────────────────────────────────────

struct AddOpts {
    source_root: PathBuf,
    repo_id: Option<String>,
    db_path: Option<PathBuf>,
    git_diff: bool,
}

fn parse_add_args(args: &[String]) -> Result<AddOpts, CliError> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("Usage: codeatlas repo add <path> [--repo-id <id>] [--db <path>] [--git-diff]");
        eprintln!();
        eprintln!("Register and index a repository. The repo_id defaults to the");
        eprintln!("directory name of <path>.");
        return Err(CliError::Usage(String::new()));
    }

    let mut source_root: Option<PathBuf> = None;
    let mut repo_id: Option<String> = None;
    let mut db_path: Option<PathBuf> = None;
    let mut git_diff = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--repo-id" => {
                i += 1;
                repo_id = Some(
                    args.get(i)
                        .ok_or_else(|| CliError::Usage("--repo-id requires a value".into()))?
                        .clone(),
                );
            }
            "--db" => {
                i += 1;
                db_path =
                    Some(PathBuf::from(args.get(i).ok_or_else(|| {
                        CliError::Usage("--db requires a value".into())
                    })?));
            }
            "--git-diff" => {
                git_diff = true;
            }
            arg if arg.starts_with('-') => {
                return Err(CliError::Usage(format!("unknown option: {arg}")));
            }
            arg if source_root.is_none() => {
                source_root = Some(PathBuf::from(arg));
            }
            other => {
                return Err(CliError::Usage(format!("unexpected argument: {other}")));
            }
        }
        i += 1;
    }

    let source_root = source_root.ok_or_else(|| {
        CliError::Usage(
            "usage: codeatlas repo add <path> [--repo-id <id>] [--db <path>] [--git-diff]".into(),
        )
    })?;

    Ok(AddOpts {
        source_root,
        repo_id,
        db_path,
        git_diff,
    })
}

fn run_add(args: &[String]) -> Result<(), CliError> {
    let opts = parse_add_args(args)?;
    let source_root = opts.source_root.canonicalize()?;

    let repo_id = match opts.repo_id {
        Some(id) => id,
        None => source_root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| {
                CliError::Usage(
                    "cannot derive repo_id from path; use --repo-id to specify one".into(),
                )
            })?,
    };

    let (db_path, blob_path) =
        cli::data_root::resolve_db_and_blob_paths(opts.db_path).map_err(CliError::Usage)?;

    // Check for repo_id collision with a different source root.
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut db = store::MetadataStore::open(&db_path)?;

    if let Some(existing) = db.repos().get(&repo_id)? {
        // Canonicalize both sides so that symlinks or relative paths stored
        // from earlier registrations don't cause false collisions.
        let existing_canonical = std::path::PathBuf::from(&existing.source_root)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(&existing.source_root));
        if existing_canonical != source_root {
            return Err(CliError::Usage(format!(
                "repo_id '{repo_id}' is already registered with a different source root:\n  \
                 existing: {}\n  \
                 new:      {}\n\n\
                 Use --repo-id <different-id> to register under a different name,\n\
                 or 'codeatlas repo remove {repo_id}' first.",
                existing.source_root,
                source_root.display(),
            )));
        }
    }

    let blob_store = store::BlobStore::open(&blob_path)?;
    let adapter_router = router::build_router(&source_root);

    let ctx = PipelineContext {
        repo_id: repo_id.clone(),
        source_root,
        router: &adapter_router,
        policy_override: None,
        correlation_id: None,
        use_git_diff: opts.git_diff,
    };

    let result = indexer::run(&ctx, &mut db, &blob_store)?;

    println!("registered: {repo_id}");
    println!("files_discovered: {}", result.metrics.files_discovered);
    println!("files_with_symbols: {}", result.metrics.files_parsed);
    println!("files_file_only: {}", result.metrics.files_file_only);
    println!("symbols_extracted: {}", result.metrics.symbols_extracted);

    Ok(())
}

// ── repo list ─────────────────────────────────────────────────────────

fn parse_db_only_args(args: &[String]) -> Result<Option<PathBuf>, CliError> {
    let mut db_path: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                db_path =
                    Some(PathBuf::from(args.get(i).ok_or_else(|| {
                        CliError::Usage("--db requires a value".into())
                    })?));
            }
            "--help" | "-h" => {
                return Err(CliError::Usage(String::new()));
            }
            other => {
                return Err(CliError::Usage(format!("unexpected argument: {other}")));
            }
        }
        i += 1;
    }

    Ok(db_path)
}

fn open_db_readonly(db_override: Option<PathBuf>) -> Result<store::MetadataStore, CliError> {
    let (db_path, _) =
        cli::data_root::resolve_db_and_blob_paths(db_override).map_err(CliError::Usage)?;

    if !db_path.exists() {
        return Err(CliError::Usage(format!(
            "database not found: {}\n\nHint: run 'codeatlas repo add <path>' to register a repository.",
            db_path.display()
        )));
    }

    Ok(store::MetadataStore::open(&db_path)?)
}

fn run_list(args: &[String]) -> Result<(), CliError> {
    let db_override = parse_db_only_args(args)?;
    let db = open_db_readonly(db_override)?;
    let repos = db.repos().list_all()?;

    if repos.is_empty() {
        println!("No repositories registered.");
        println!();
        println!("Add a repository with: codeatlas repo add <path>");
        return Ok(());
    }

    for repo in &repos {
        println!(
            "{:<20} {:<8} {:<6} {:>5} files  {:>6} symbols  {}",
            repo.repo_id,
            repo.indexing_status.as_str(),
            repo.freshness_status.as_str(),
            repo.file_count,
            repo.symbol_count,
            repo.source_root,
        );
    }

    Ok(())
}

// ── repo status ───────────────────────────────────────────────────────

fn run_status(args: &[String]) -> Result<(), CliError> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("Usage: codeatlas repo status <repo_id> [--db <path>]");
        return Err(CliError::Usage(String::new()));
    }

    let mut repo_id: Option<String> = None;
    let mut db_path: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                db_path =
                    Some(PathBuf::from(args.get(i).ok_or_else(|| {
                        CliError::Usage("--db requires a value".into())
                    })?));
            }
            arg if arg.starts_with('-') => {
                return Err(CliError::Usage(format!("unknown option: {arg}")));
            }
            arg if repo_id.is_none() => {
                repo_id = Some(arg.to_string());
            }
            other => {
                return Err(CliError::Usage(format!("unexpected argument: {other}")));
            }
        }
        i += 1;
    }

    let repo_id = repo_id.ok_or_else(|| {
        CliError::Usage("usage: codeatlas repo status <repo_id> [--db <path>]".into())
    })?;

    let db = open_db_readonly(db_path)?;
    let repo = db
        .repos()
        .get(&repo_id)?
        .ok_or_else(|| CliError::Usage(format!("repo '{repo_id}' not found")))?;

    println!("repo_id:          {}", repo.repo_id);
    println!("display_name:     {}", repo.display_name);
    println!("source_root:      {}", repo.source_root);
    println!("indexing_status:  {}", repo.indexing_status.as_str());
    println!("freshness_status: {}", repo.freshness_status.as_str());
    println!("indexed_at:       {}", repo.indexed_at);
    println!("index_version:    {}", repo.index_version);
    println!("file_count:       {}", repo.file_count);
    println!("symbol_count:     {}", repo.symbol_count);
    if let Some(ref registered_at) = repo.registered_at {
        println!("registered_at:    {}", registered_at);
    }
    if let Some(ref git_head) = repo.git_head {
        println!("git_head:         {}", git_head);
    }
    if !repo.language_counts.is_empty() {
        println!("languages:");
        for (lang, count) in &repo.language_counts {
            println!("  {lang}: {count}");
        }
    }

    Ok(())
}

// ── repo refresh ──────────────────────────────────────────────────────

fn run_refresh(args: &[String]) -> Result<(), CliError> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("Usage: codeatlas repo refresh <repo_id> [--db <path>] [--git-diff]");
        return Err(CliError::Usage(String::new()));
    }

    let mut repo_id: Option<String> = None;
    let mut db_path: Option<PathBuf> = None;
    let mut git_diff = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                db_path =
                    Some(PathBuf::from(args.get(i).ok_or_else(|| {
                        CliError::Usage("--db requires a value".into())
                    })?));
            }
            "--git-diff" => {
                git_diff = true;
            }
            arg if arg.starts_with('-') => {
                return Err(CliError::Usage(format!("unknown option: {arg}")));
            }
            arg if repo_id.is_none() => {
                repo_id = Some(arg.to_string());
            }
            other => {
                return Err(CliError::Usage(format!("unexpected argument: {other}")));
            }
        }
        i += 1;
    }

    let repo_id = repo_id.ok_or_else(|| {
        CliError::Usage("usage: codeatlas repo refresh <repo_id> [--db <path>] [--git-diff]".into())
    })?;

    let (db_path, blob_path) =
        cli::data_root::resolve_db_and_blob_paths(db_path).map_err(CliError::Usage)?;

    if !db_path.exists() {
        return Err(CliError::Usage(format!(
            "database not found: {}",
            db_path.display()
        )));
    }

    let mut db = store::MetadataStore::open(&db_path)?;

    let repo = db
        .repos()
        .get(&repo_id)?
        .ok_or_else(|| CliError::Usage(format!("repo '{repo_id}' not found")))?;

    let source_root = PathBuf::from(&repo.source_root);
    if !source_root.is_dir() {
        return Err(CliError::Usage(format!(
            "source root is not accessible: {}",
            repo.source_root
        )));
    }

    let blob_store = store::BlobStore::open(&blob_path)?;
    let adapter_router = router::build_router(&source_root);

    let ctx = PipelineContext {
        repo_id: repo_id.clone(),
        source_root,
        router: &adapter_router,
        policy_override: None,
        correlation_id: None,
        use_git_diff: git_diff,
    };

    let result = indexer::run(&ctx, &mut db, &blob_store)?;

    println!("refreshed: {repo_id}");
    println!("files_discovered: {}", result.metrics.files_discovered);
    println!("files_with_symbols: {}", result.metrics.files_parsed);
    println!("files_file_only: {}", result.metrics.files_file_only);
    println!("files_unchanged: {}", result.metrics.files_unchanged);
    println!("files_deleted: {}", result.metrics.files_deleted);
    println!("symbols_extracted: {}", result.metrics.symbols_extracted);

    Ok(())
}

// ── repo remove ───────────────────────────────────────────────────────

fn run_remove(args: &[String]) -> Result<(), CliError> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("Usage: codeatlas repo remove <repo_id> [--db <path>]");
        return Err(CliError::Usage(String::new()));
    }

    let mut repo_id: Option<String> = None;
    let mut db_path: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                db_path =
                    Some(PathBuf::from(args.get(i).ok_or_else(|| {
                        CliError::Usage("--db requires a value".into())
                    })?));
            }
            arg if arg.starts_with('-') => {
                return Err(CliError::Usage(format!("unknown option: {arg}")));
            }
            arg if repo_id.is_none() => {
                repo_id = Some(arg.to_string());
            }
            other => {
                return Err(CliError::Usage(format!("unexpected argument: {other}")));
            }
        }
        i += 1;
    }

    let repo_id = repo_id.ok_or_else(|| {
        CliError::Usage("usage: codeatlas repo remove <repo_id> [--db <path>]".into())
    })?;

    let (db_path, blob_path) =
        cli::data_root::resolve_db_and_blob_paths(db_path).map_err(CliError::Usage)?;

    if !db_path.exists() {
        return Err(CliError::Usage(format!(
            "database not found: {}",
            db_path.display()
        )));
    }

    let db = store::MetadataStore::open(&db_path)?;

    // Verify the repo exists before collecting hashes.
    if db.repos().get(&repo_id)?.is_none() {
        return Err(CliError::Usage(format!("repo '{repo_id}' not found")));
    }

    // Collect file hashes before deletion so we can clean up orphaned blobs.
    let hashes = db.files().list_hashes(&repo_id)?;

    // Delete repo metadata (cascades to files and symbols via ON DELETE CASCADE).
    db.repos().delete(&repo_id)?;

    // Remove blobs that are no longer referenced by any remaining file.
    if blob_path.is_dir() {
        let blob_store = store::BlobStore::open(&blob_path)?;
        let mut blobs_removed = 0u64;
        let mut blob_errors = 0u64;
        for hash in &hashes {
            if !db.files().is_hash_referenced(hash)? {
                match blob_store.delete(hash) {
                    Ok(true) => blobs_removed += 1,
                    Ok(false) => {} // blob already absent, nothing to do
                    Err(e) => {
                        eprintln!("warning: failed to delete blob {hash}: {e}");
                        blob_errors += 1;
                    }
                }
            }
        }
        if blob_errors > 0 {
            println!(
                "removed: {repo_id} ({blobs_removed} blobs cleaned up, \
                 {blob_errors} blob deletions failed)"
            );
        } else if blobs_removed > 0 {
            println!("removed: {repo_id} ({blobs_removed} orphaned blobs cleaned up)");
        } else {
            println!("removed: {repo_id}");
        }
    } else {
        println!("removed: {repo_id}");
    }

    Ok(())
}

// ── Help ──────────────────────────────────────────────────────────────

const REPO_HELP: &str = "\
Usage: codeatlas repo <subcommand> [options]

Subcommands:
  add       Register and index a repository
  list      List all registered repositories
  status    Show detailed status for a repository
  refresh   Re-index a registered repository
  remove    De-register a repository and delete its data";

fn print_repo_help() {
    eprintln!("{REPO_HELP}");
}
