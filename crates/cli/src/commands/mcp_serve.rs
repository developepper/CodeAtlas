//! `codeatlas mcp serve` command — start the MCP tool server over stdio.

use std::path::PathBuf;

use query_engine::StoreQueryService;
use server_mcp::ToolRegistry;

use crate::error::CliError;

/// Entry point for the `codeatlas mcp` command family.
///
/// Dispatches to `serve` (or future subcommands). Prints mcp-level help
/// when invoked with `--help` / `-h` or without a subcommand.
pub fn run(args: &[String]) -> Result<(), CliError> {
    let subcommand = args.first().map(|s| s.as_str());
    match subcommand {
        Some("serve") => run_serve(&args[1..]),
        Some("--help" | "-h") => {
            print_mcp_help();
            Ok(())
        }
        Some(other) => Err(CliError::Usage(format!(
            "unknown mcp subcommand: '{other}'\n\nUsage: codeatlas mcp <subcommand>\n\nSubcommands:\n  serve    Validate MCP server startup (transport pending #131)"
        ))),
        None => {
            print_mcp_help();
            Err(CliError::Usage(
                "missing mcp subcommand".into(),
            ))
        }
    }
}

/// `codeatlas mcp serve --db <path>`
///
/// Validates the database path, opens the store, and creates the tool
/// registry. Once #131 lands, this will start the stdio JSON-RPC loop.
/// Until then the command validates startup preconditions and exits
/// non-zero so that callers do not mistake it for a running server.
fn run_serve(args: &[String]) -> Result<(), CliError> {
    // Handle --help before parsing to exit cleanly with code 0.
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_serve_help();
        return Ok(());
    }

    let opts = parse_serve_args(args)?;

    // Validate DB path exists before attempting to open.
    if !opts.db_path.exists() {
        return Err(CliError::Usage(format!(
            "database not found: {}\n\nHint: run 'codeatlas index <repo>' first to create the index database.",
            opts.db_path.display()
        )));
    }

    // Open store — propagates StoreError on schema mismatch or corruption.
    let db = store::MetadataStore::open(&opts.db_path)?;
    let svc = StoreQueryService::new(&db);
    let _registry = ToolRegistry::new(&svc);

    // Startup preconditions passed — DB is valid and registry is wired.
    eprintln!(
        "codeatlas mcp: validated db={} ({} tools registered)",
        opts.db_path.display(),
        _registry.tool_names().len(),
    );

    // The stdio JSON-RPC transport loop is not yet implemented (#131).
    // Exit non-zero so MCP clients and scripts do not mistake a
    // successful exit for a server that ran and closed cleanly.
    Err(CliError::Usage(
        "stdio transport not yet implemented (see #131)".into(),
    ))
}

// ── Argument parsing ───────────────────────────────────────────────────

#[derive(Debug)]
struct ServeOpts {
    db_path: PathBuf,
}

fn parse_serve_args(args: &[String]) -> Result<ServeOpts, CliError> {
    let mut db_path: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                db_path = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| CliError::Usage("--db requires a value".into()))?,
                ));
            }
            other => {
                return Err(CliError::Usage(format!("unknown option: {other}")));
            }
        }
        i += 1;
    }

    let db_path = db_path.ok_or_else(|| {
        CliError::Usage(
            "--db <path> is required\n\nUsage: codeatlas mcp serve --db <path>".into(),
        )
    })?;

    Ok(ServeOpts { db_path })
}

// ── Help text ──────────────────────────────────────────────────────────

fn print_mcp_help() {
    eprintln!("Usage: codeatlas mcp <subcommand>");
    eprintln!();
    eprintln!("Subcommands:");
    eprintln!("  serve    Validate MCP server startup (transport pending #131)");
    eprintln!();
    eprintln!("Run 'codeatlas mcp serve --help' for serve options.");
}

fn print_serve_help() {
    eprintln!("Usage: codeatlas mcp serve --db <path>");
    eprintln!();
    eprintln!("Validate startup preconditions for the MCP tool server.");
    eprintln!("The stdio transport is not yet available (see #131).");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --db <path>    Path to the CodeAtlas index database (required)");
    eprintln!("                 Typically: <repo>/.codeatlas/index.db");
    eprintln!();
    eprintln!("Repository-scoped tools accept repo_id as a parameter in each tool");
    eprintln!("call. The repo_id is derived from the indexed directory name (e.g.,");
    eprintln!("indexing /home/user/my-app produces repo_id 'my-app').");
    eprintln!();
    eprintln!("When the transport lands, all diagnostics will be written to stderr.");
    eprintln!("Stdout will be reserved for MCP protocol frames.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_valid_db_path() {
        let a = args(&["--db", "/tmp/test.db"]);
        let opts = parse_serve_args(&a).unwrap();
        assert_eq!(opts.db_path, PathBuf::from("/tmp/test.db"));
    }

    #[test]
    fn parse_missing_db_flag() {
        let a: Vec<String> = vec![];
        let err = parse_serve_args(&a).unwrap_err();
        assert!(err.to_string().contains("--db <path> is required"));
    }

    #[test]
    fn parse_db_missing_value() {
        let a = args(&["--db"]);
        let err = parse_serve_args(&a).unwrap_err();
        assert!(err.to_string().contains("--db requires a value"));
    }

    #[test]
    fn parse_unknown_option() {
        let a = args(&["--db", "/tmp/test.db", "--verbose"]);
        let err = parse_serve_args(&a).unwrap_err();
        assert!(err.to_string().contains("unknown option: --verbose"));
    }

    #[test]
    fn run_missing_subcommand() {
        let a: Vec<String> = vec![];
        let err = run(&a).unwrap_err();
        assert!(err.to_string().contains("missing mcp subcommand"));
    }

    #[test]
    fn run_unknown_subcommand() {
        let a = args(&["start"]);
        let err = run(&a).unwrap_err();
        assert!(err.to_string().contains("unknown mcp subcommand: 'start'"));
    }

    #[test]
    fn serve_nonexistent_db() {
        let a = args(&["serve", "--db", "/nonexistent/path/to/db.sqlite"]);
        let err = run(&a).unwrap_err();
        assert!(err.to_string().contains("database not found"));
    }

    #[test]
    fn serve_help_flag() {
        let a = args(&["serve", "--help"]);
        // Should return Ok (not an error) for explicit help.
        assert!(run(&a).is_ok());
    }

    #[test]
    fn mcp_help_flag() {
        let a = args(&["--help"]);
        assert!(run(&a).is_ok());
    }

    #[test]
    fn serve_valid_db_exits_not_implemented() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("index.db");
        // Create a real store so the DB file exists and is valid.
        let _db = store::MetadataStore::open(&db_path).unwrap();
        drop(_db);

        let a = args(&["serve", "--db", db_path.to_str().unwrap()]);
        let err = run(&a).unwrap_err();
        assert!(
            err.to_string().contains("not yet implemented"),
            "expected 'not yet implemented' error, got: {err}"
        );
    }
}
