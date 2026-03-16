//! `codeatlas get-symbol` command — retrieves a single symbol by ID.

use std::path::PathBuf;

use query_engine::{QueryService, StoreQueryService};

use crate::error::CliError;

pub fn run(args: &[String]) -> Result<(), CliError> {
    let (db_path, symbol_id) = parse_args(args)?;

    let db = store::MetadataStore::open(&db_path)?;
    let blob_path = db_path
        .parent()
        .map(|p| p.join("blobs"))
        .unwrap_or_else(|| PathBuf::from("blobs"));
    let blob_store = store::BlobStore::open(&blob_path)?;
    let svc = StoreQueryService::new(&db, &blob_store);

    let record = svc.get_symbol(&symbol_id)?;

    println!("id: {}", record.id);
    println!("name: {}", record.name);
    println!("kind: {}", record.kind.as_str());
    println!("qualified_name: {}", record.qualified_name);
    println!("file: {}:{}", record.file_path, record.start_line);
    println!("language: {}", record.language);
    println!("signature: {}", record.signature);
    println!("tier: {}", record.capability_tier);
    println!("confidence: {}", record.confidence_score);
    if let Some(ref doc) = record.docstring {
        println!("docstring: {doc}");
    }

    Ok(())
}

fn parse_args(args: &[String]) -> Result<(PathBuf, String), CliError> {
    let mut db_path: Option<PathBuf> = None;
    let mut symbol_id: Option<String> = None;

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
            arg if !arg.starts_with('-') && symbol_id.is_none() => {
                symbol_id = Some(arg.to_string());
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
    let symbol_id = symbol_id.ok_or_else(|| CliError::Usage("symbol ID is required".into()))?;

    Ok((db_path, symbol_id))
}
