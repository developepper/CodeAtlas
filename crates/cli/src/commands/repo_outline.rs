//! `codeatlas repo-outline` command — shows repository structure overview.

use std::path::PathBuf;

use query_engine::{QueryService, RepoOutlineRequest, StoreQueryService};

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

    let outline = svc.get_repo_outline(&RepoOutlineRequest {
        repo_id: opts.repo_id,
    })?;

    println!("repo_id: {}", outline.repo.repo_id);
    println!("display_name: {}", outline.repo.display_name);
    println!("file_count: {}", outline.repo.file_count);
    println!("symbol_count: {}", outline.repo.symbol_count);
    println!("files:");

    for f in &outline.files {
        println!(
            "  - path: {}\n    language: {}\n    symbols: {}",
            f.path, f.language, f.symbol_count,
        );
    }

    Ok(())
}

struct RepoOutlineOpts {
    db_path: PathBuf,
    repo_id: String,
}

fn parse_args(args: &[String]) -> Result<RepoOutlineOpts, CliError> {
    let mut db_path: Option<PathBuf> = None;
    let mut repo_id: Option<String> = None;

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

    Ok(RepoOutlineOpts { db_path, repo_id })
}
