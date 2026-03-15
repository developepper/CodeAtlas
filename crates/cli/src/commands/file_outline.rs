//! `codeatlas file-outline` command — lists symbols in a file.

use std::path::PathBuf;

use query_engine::{FileOutlineRequest, QueryService, StoreQueryService};

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

    let outline = svc.get_file_outline(&FileOutlineRequest {
        repo_id: opts.repo_id,
        file_path: opts.file_path,
    })?;

    println!("file: {}", outline.file.file_path);
    println!("language: {}", outline.file.language);
    println!("symbol_count: {}", outline.file.symbol_count);
    println!("symbols:");

    for s in &outline.symbols {
        println!(
            "  - name: {}\n    kind: {}\n    line: {}",
            s.name,
            s.kind.as_str(),
            s.start_line,
        );
    }

    Ok(())
}

struct FileOutlineOpts {
    db_path: PathBuf,
    repo_id: String,
    file_path: String,
}

fn parse_args(args: &[String]) -> Result<FileOutlineOpts, CliError> {
    let mut db_path: Option<PathBuf> = None;
    let mut repo_id: Option<String> = None;
    let mut file_path: Option<String> = None;

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
            arg if !arg.starts_with('-') && file_path.is_none() => {
                file_path = Some(arg.to_string());
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
    let file_path =
        file_path.ok_or_else(|| CliError::Usage("file path argument is required".into()))?;

    Ok(FileOutlineOpts {
        db_path,
        repo_id,
        file_path,
    })
}
