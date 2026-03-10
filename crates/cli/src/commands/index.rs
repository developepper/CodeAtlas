//! `codeatlas index` command — indexes a repository.

use std::path::PathBuf;

use adapter_api::AdapterPolicy;
use indexer::PipelineContext;

use crate::error::CliError;
use crate::router::TreeSitterRouter;

struct IndexOpts {
    source_root: PathBuf,
    db_path: Option<PathBuf>,
    git_diff: bool,
}

fn parse_args(args: &[String]) -> Result<IndexOpts, CliError> {
    let mut source_root: Option<PathBuf> = None;
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
            arg if source_root.is_none() => {
                source_root = Some(PathBuf::from(arg));
            }
            other => {
                return Err(CliError::Usage(format!("unexpected argument: {other}")));
            }
        }
        i += 1;
    }

    let source_root = source_root
        .ok_or_else(|| CliError::Usage("usage: codeatlas index <path> [--db <path>]".into()))?;

    Ok(IndexOpts {
        source_root,
        db_path,
        git_diff,
    })
}

pub fn run(args: &[String]) -> Result<(), CliError> {
    let opts = parse_args(args)?;

    let source_root = opts.source_root.canonicalize()?;
    let db_path = opts
        .db_path
        .unwrap_or_else(|| source_root.join(".codeatlas").join("index.db"));
    let blob_path = db_path
        .parent()
        .map_or_else(|| PathBuf::from(".codeatlas/blobs"), |p| p.join("blobs"));

    // Derive repo_id from directory name.
    let repo_id = source_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".into());

    // Ensure parent directories exist.
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut db = store::MetadataStore::open(&db_path)?;
    let blob_store = store::BlobStore::open(&blob_path)?;
    let router = TreeSitterRouter::new();

    let ctx = PipelineContext {
        repo_id,
        source_root,
        router: &router,
        default_policy: AdapterPolicy::SyntaxOnly,
        correlation_id: None,
        use_git_diff: opts.git_diff,
    };

    let result = indexer::run(&ctx, &mut db, &blob_store)?;

    println!("files_discovered: {}", result.metrics.files_discovered);
    println!("files_parsed: {}", result.metrics.files_parsed);
    println!("files_unchanged: {}", result.metrics.files_unchanged);
    println!("files_deleted: {}", result.metrics.files_deleted);
    println!("files_errored: {}", result.metrics.files_errored);
    println!("symbols_extracted: {}", result.metrics.symbols_extracted);

    if !result.file_errors.is_empty() {
        eprintln!();
        eprintln!("file errors:");
        for err in &result.file_errors {
            eprintln!("  {}: {}", err.path.display(), err.error);
        }
    }

    Ok(())
}
