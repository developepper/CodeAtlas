//! `codeatlas quality-report <path>` — generates a quality KPI report.
//!
//! Indexes the given repository and reports semantic coverage metrics,
//! win-rate KPIs, and per-adapter breakdowns from the actual parse output.
//!
//! For fixture-based regression KPIs, see the CI `rust-quality-kpi` job
//! which runs the adapter regression suites and uploads a report artifact.

use std::path::PathBuf;

use indexer::PipelineContext;

use crate::error::CliError;
use crate::router;

struct ReportOpts {
    source_root: PathBuf,
    db_path: Option<PathBuf>,
    git_diff: bool,
}

fn parse_args(args: &[String]) -> Result<ReportOpts, CliError> {
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

    let source_root = source_root.ok_or_else(|| {
        CliError::Usage("usage: codeatlas quality-report <path> [--db <path>] [--git-diff]".into())
    })?;

    Ok(ReportOpts {
        source_root,
        db_path,
        git_diff,
    })
}

pub fn run(args: &[String]) -> Result<(), CliError> {
    let opts = parse_args(args)?;

    let source_root = opts.source_root.canonicalize()?;
    let (db_path, blob_path) =
        cli::data_root::resolve_db_and_blob_paths(opts.db_path).map_err(CliError::Usage)?;

    let repo_id = source_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".into());

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut db = store::MetadataStore::open(&db_path)?;
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
    let c = &result.metrics.coverage;

    // Header
    println!("Quality KPI Report");
    println!("==================");
    println!();

    // Repository overview
    println!("Repository: {repo_id}");
    println!(
        "Files:      {} discovered, {} with symbols, {} file-only, {} errored",
        result.metrics.files_discovered,
        result.metrics.files_parsed,
        result.metrics.files_file_only,
        result.metrics.files_errored,
    );
    println!("Symbols:    {}", result.metrics.symbols_extracted);
    let total_indexed = result.metrics.files_parsed + result.metrics.files_file_only;
    if total_indexed > 0 {
        println!(
            "Index coverage: {} of {} discovered files indexed ({} with symbols, {} file-only)",
            total_indexed,
            result.metrics.files_discovered,
            result.metrics.files_parsed,
            result.metrics.files_file_only,
        );
    }
    println!();

    // Semantic coverage
    println!("Semantic Coverage");
    println!("-----------------");
    println!("Total symbols:          {}", c.total_symbols);
    println!("Semantic symbols:       {}", c.semantic_symbols);
    println!("Syntax symbols:         {}", c.syntax_symbols);
    println!(
        "Coverage:               {:.1}%",
        c.semantic_coverage_percent
    );
    println!("Avg confidence:         {:.3}", c.avg_confidence);
    println!(
        "Files with semantic:    {}/{}",
        c.files_with_semantic, c.total_files
    );
    if c.duplicates_resolved > 0 {
        println!("Duplicates resolved:    {}", c.duplicates_resolved);
    }
    println!();

    // Win rate
    let overlap_total = c.wins + c.losses + c.ties;
    println!("Semantic vs Syntax Win Rate");
    println!("--------------------------");
    if overlap_total > 0 {
        println!("Win rate:               {:.1}%", c.win_rate * 100.0);
        println!("Wins:                   {}", c.wins);
        println!("Losses:                 {}", c.losses);
        println!("Ties:                   {}", c.ties);
    } else {
        println!("(no overlapping symbols — single adapter per file)");
    }
    println!();

    // Per-adapter breakdown
    if !c.adapter_symbol_counts.is_empty() {
        println!("Adapter Breakdown");
        println!("-----------------");
        for (adapter, count) in &c.adapter_symbol_counts {
            let pct = if c.total_symbols > 0 {
                (*count as f32 / c.total_symbols as f32) * 100.0
            } else {
                0.0
            };
            println!("{adapter}: {count} ({pct:.1}%)");
        }
        println!();
    }

    // Pass/fail summary
    println!("KPI Status");
    println!("----------");

    // Symbol quality KPIs are not applicable when no symbols were extracted.
    // File-only indexing is useful but should not masquerade as a passing
    // symbol quality gate.
    if c.total_symbols == 0 {
        if result.metrics.files_file_only > 0 {
            println!(
                "[INFO] No symbols extracted ({} files indexed at file level only)",
                result.metrics.files_file_only
            );
            println!("[INFO] Symbol quality KPIs are not applicable for file-only repos");
        } else {
            println!("[WARN] No symbols extracted and no files indexed");
        }
        println!();
        println!("Result: NOT APPLICABLE");
    } else {
        let mut all_pass = true;

        println!("[PASS] Symbols extracted: {}", c.total_symbols);

        if c.avg_confidence >= 0.85 {
            println!("[PASS] Avg confidence: {:.3} (>= 0.850)", c.avg_confidence);
        } else {
            println!(
                "[WARN] Avg confidence: {:.3} (below 0.850)",
                c.avg_confidence
            );
        }

        if c.losses == 0 {
            println!("[PASS] No semantic-vs-syntax losses");
        } else {
            println!("[FAIL] {} semantic-vs-syntax losses", c.losses);
            all_pass = false;
        }

        if overlap_total > 0 && c.win_rate >= 0.8 {
            println!("[PASS] Win rate: {:.1}% (>= 80.0%)", c.win_rate * 100.0);
        } else if overlap_total > 0 {
            println!("[FAIL] Win rate: {:.1}% (below 80.0%)", c.win_rate * 100.0);
            all_pass = false;
        }

        println!();
        if all_pass {
            println!("Result: PASS");
        } else {
            println!("Result: FAIL");
        }
    }

    if !result.file_errors.is_empty() {
        eprintln!();
        eprintln!("File errors:");
        for err in &result.file_errors {
            eprintln!("  {}: {}", err.path.display(), err.error);
        }
    }

    Ok(())
}
