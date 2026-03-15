//! `codeatlas file-tree` command — lists files in a repository.

use std::path::PathBuf;

use query_engine::{FileTreeRequest, QueryService, StoreQueryService};

use crate::error::CliError;

pub fn run(args: &[String]) -> Result<(), CliError> {
    let opts = parse_args(args)?;

    let db = store::MetadataStore::open(&opts.db_path)?;
    let blob_path = opts
        .db_path
        .parent()
        .map(|p| p.join("blobs"))
        .unwrap_or_else(|| PathBuf::from("blobs"));
    let blob_store = store::BlobStore::open(&blob_path)?;
    let svc = StoreQueryService::new(&db, &blob_store);

    let entries = svc.get_file_tree(&FileTreeRequest {
        repo_id: opts.repo_id,
        path_prefix: opts.path_prefix,
    })?;

    println!("entries: {}", entries.len());
    for e in &entries {
        println!(
            "  - path: {}\n    language: {}\n    symbols: {}",
            e.path, e.language, e.symbol_count
        );
    }

    Ok(())
}

struct FileTreeOpts {
    db_path: PathBuf,
    repo_id: String,
    path_prefix: Option<String>,
}

fn parse_args(args: &[String]) -> Result<FileTreeOpts, CliError> {
    let mut db_path: Option<PathBuf> = None;
    let mut repo_id: Option<String> = None;
    let mut path_prefix: Option<String> = None;

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
            "--repo" => {
                i += 1;
                repo_id = Some(
                    args.get(i)
                        .ok_or_else(|| CliError::Usage("--repo requires a value".into()))?
                        .clone(),
                );
            }
            "--prefix" => {
                i += 1;
                path_prefix = Some(
                    args.get(i)
                        .ok_or_else(|| CliError::Usage("--prefix requires a value".into()))?
                        .clone(),
                );
            }
            other => {
                return Err(CliError::Usage(format!("unknown option: {other}")));
            }
        }
        i += 1;
    }

    let db_path = match db_path {
        Some(p) => p,
        None => cli::data_root::default_db_path().map_err(CliError::Usage)?,
    };
    let repo_id = repo_id.ok_or_else(|| CliError::Usage("--repo <id> is required".into()))?;

    Ok(FileTreeOpts {
        db_path,
        repo_id,
        path_prefix,
    })
}
