//! `codeatlas search-symbols` command — ranked symbol search.

use std::path::PathBuf;

use core_model::SymbolKind;
use query_engine::{QueryFilters, QueryService, StoreQueryService, SymbolQuery};

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

    let query = SymbolQuery {
        repo_id: opts.repo_id,
        text: opts.query,
        filters: QueryFilters {
            kind: opts.kind,
            language: opts.language,
            capability_tier: None,
            file_path: None,
        },
        limit: opts.limit,
        offset: 0,
    };

    let result = svc.search_symbols(&query)?;

    println!("total_candidates: {}", result.meta.total_candidates);
    println!("truncated: {}", result.meta.truncated);
    println!("results:");

    for scored in &result.items {
        let s = &scored.record;
        println!(
            "  - id: {}\n    name: {}\n    kind: {}\n    file: {}:{}\n    score: {:.3}",
            s.id,
            s.name,
            s.kind.as_str(),
            s.file_path,
            s.start_line,
            scored.score,
        );
    }

    Ok(())
}

struct SearchOpts {
    db_path: PathBuf,
    repo_id: String,
    query: String,
    kind: Option<SymbolKind>,
    language: Option<String>,
    limit: usize,
}

fn parse_args(args: &[String]) -> Result<SearchOpts, CliError> {
    let mut db_path: Option<PathBuf> = None;
    let mut repo_id: Option<String> = None;
    let mut query: Option<String> = None;
    let mut kind: Option<SymbolKind> = None;
    let mut language: Option<String> = None;
    let mut limit: usize = 20;

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
            "--kind" => {
                i += 1;
                let val = args
                    .get(i)
                    .ok_or_else(|| CliError::Usage("--kind requires a value".into()))?;
                kind = Some(parse_symbol_kind(val)?);
            }
            "--language" => {
                i += 1;
                language = Some(
                    args.get(i)
                        .ok_or_else(|| CliError::Usage("--language requires a value".into()))?
                        .clone(),
                );
            }
            "--limit" => {
                i += 1;
                let val = args
                    .get(i)
                    .ok_or_else(|| CliError::Usage("--limit requires a value".into()))?;
                limit = val
                    .parse()
                    .map_err(|_| CliError::Usage(format!("invalid limit: {val}")))?;
            }
            arg if !arg.starts_with('-') && query.is_none() => {
                query = Some(arg.to_string());
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
    let query = query.ok_or_else(|| CliError::Usage("search query text is required".into()))?;

    Ok(SearchOpts {
        db_path,
        repo_id,
        query,
        kind,
        language,
        limit,
    })
}

fn parse_symbol_kind(s: &str) -> Result<SymbolKind, CliError> {
    match s.to_lowercase().as_str() {
        "function" => Ok(SymbolKind::Function),
        "class" => Ok(SymbolKind::Class),
        "method" => Ok(SymbolKind::Method),
        "type" => Ok(SymbolKind::Type),
        "constant" => Ok(SymbolKind::Constant),
        other => Err(CliError::Usage(format!(
            "unknown symbol kind: {other} (expected: function, class, method, type, constant)"
        ))),
    }
}
