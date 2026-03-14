//! `codeatlas mcp serve` command — start the MCP tool server over stdio.

use std::path::PathBuf;

use query_engine::StoreQueryService;
use server_mcp::ToolRegistry;

use crate::error::CliError;
use crate::mcp_stdio;

/// Entry point for the `codeatlas mcp` command family.
///
/// Dispatches to `serve` (or future subcommands). Prints mcp-level help
/// when invoked with `--help` / `-h` or without a subcommand.
pub fn run(args: &[String]) -> Result<(), CliError> {
    let subcommand = args.first().map(|s| s.as_str());
    match subcommand {
        Some("serve") => run_serve(&args[1..]),
        Some("bridge") => run_bridge(&args[1..]),
        Some("--help" | "-h") => {
            print_mcp_help();
            Ok(())
        }
        Some(other) => Err(CliError::Usage(format!(
            "unknown mcp subcommand: '{other}'\n\nUsage: codeatlas mcp <subcommand>\n\nSubcommands:\n  serve     Start the MCP tool server (stdio, direct store)\n  bridge    Start the MCP bridge to the persistent service"
        ))),
        None => {
            print_mcp_help();
            Err(CliError::Usage("missing mcp subcommand".into()))
        }
    }
}

/// `codeatlas mcp serve [--db <path>]`
///
/// Validates the database path, opens the store, creates the tool
/// registry, and runs the stdio JSON-RPC server loop. All diagnostics
/// go to stderr; stdout is reserved for MCP protocol messages.
fn run_serve(args: &[String]) -> Result<(), CliError> {
    // Handle --help before parsing to exit cleanly with code 0.
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_serve_help();
        return Ok(());
    }

    let opts = parse_serve_args(args)?;

    // Validate DB path exists and is readable before attempting to open.
    if !opts.db_path.exists() {
        return Err(CliError::Usage(format!(
            "database not found: {}\n\nHint: run 'codeatlas index <repo>' first to create the index database.",
            opts.db_path.display()
        )));
    }

    if opts.db_path.is_dir() {
        return Err(CliError::Usage(format!(
            "database path is a directory, not a file: {}",
            opts.db_path.display()
        )));
    }

    // Check read permissions before opening to produce a clear diagnostic
    // rather than a raw SQLite error.
    match std::fs::File::open(&opts.db_path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(CliError::Usage(format!(
                "database is not readable: {}\n\nHint: check file permissions.",
                opts.db_path.display()
            )));
        }
        Err(e) => {
            return Err(CliError::Usage(format!(
                "cannot open database: {}: {}",
                opts.db_path.display(),
                e
            )));
        }
    }

    // Open store — propagates StoreError on schema mismatch or corruption.
    let db = match store::MetadataStore::open(&opts.db_path) {
        Ok(db) => db,
        Err(e) => {
            return Err(CliError::Usage(format!(
                "failed to open database: {}\n\nThe file exists but could not be opened as a CodeAtlas index: {}",
                opts.db_path.display(),
                e
            )));
        }
    };
    let svc = StoreQueryService::new(&db);
    let registry = ToolRegistry::new(&svc);

    eprintln!(
        "codeatlas mcp: serving db={} ({} tools registered)",
        opts.db_path.display(),
        registry.tool_names().len(),
    );

    // Install signal handlers before entering the server loop so that
    // SIGTERM/SIGINT trigger a clean shutdown without partial writes.
    mcp_stdio::install_signal_handlers();

    // Run the stdio JSON-RPC server loop. Returns on EOF or fatal error.
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    mcp_stdio::serve(&registry, stdin.lock(), stdout.lock())
        .map_err(|e| CliError::Usage(format!("mcp server error: {e}")))
}

/// `codeatlas mcp bridge [--service-url <addr>]`
///
/// Validates that the service is reachable, then runs the MCP bridge
/// stdio loop. All diagnostics go to stderr; stdout is reserved for
/// MCP protocol messages.
fn run_bridge(args: &[String]) -> Result<(), CliError> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_bridge_help();
        return Ok(());
    }

    let opts = parse_bridge_args(args)?;

    eprintln!(
        "codeatlas mcp bridge: connecting to service at {}",
        opts.service_addr
    );

    crate::mcp_bridge::install_signal_handlers();

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    crate::mcp_bridge::serve_bridge(&opts.service_addr, stdin.lock(), stdout.lock())
        .map_err(|e| CliError::Io(std::io::Error::other(format!("mcp bridge: {e}"))))
}

#[derive(Debug)]
struct BridgeOpts {
    service_addr: String,
}

fn parse_bridge_args(args: &[String]) -> Result<BridgeOpts, CliError> {
    let mut service_addr: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--service-url" => {
                i += 1;
                service_addr = Some(
                    args.get(i)
                        .ok_or_else(|| CliError::Usage("--service-url requires a value".into()))?
                        .clone(),
                );
            }
            other => {
                return Err(CliError::Usage(format!("unknown option: {other}")));
            }
        }
        i += 1;
    }

    // Default to the canonical service address.
    let service_addr = match service_addr {
        Some(addr) => addr,
        None => {
            let port = std::env::var("CODEATLAS_PORT")
                .ok()
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(service::DEFAULT_PORT);
            let host = std::env::var("CODEATLAS_HOST")
                .ok()
                .and_then(|v| v.parse::<std::net::IpAddr>().ok())
                .unwrap_or(service::DEFAULT_HOST);
            format!("{host}:{port}")
        }
    };

    Ok(BridgeOpts { service_addr })
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
                db_path =
                    Some(PathBuf::from(args.get(i).ok_or_else(|| {
                        CliError::Usage("--db requires a value".into())
                    })?));
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

    Ok(ServeOpts { db_path })
}

// ── Help text ──────────────────────────────────────────────────────────

fn print_mcp_help() {
    eprintln!("Usage: codeatlas mcp <subcommand>");
    eprintln!();
    eprintln!("Subcommands:");
    eprintln!("  serve     Start the MCP tool server (stdio, direct store)");
    eprintln!("  bridge    Start the MCP bridge to the persistent service");
    eprintln!();
    eprintln!("Run 'codeatlas mcp <subcommand> --help' for options.");
}

fn print_bridge_help() {
    eprintln!("Usage: codeatlas mcp bridge [--service-url <host:port>]");
    eprintln!();
    eprintln!("Start an MCP bridge that proxies tool calls to the persistent");
    eprintln!("CodeAtlas HTTP service. AI clients launch this as their MCP command");
    eprintln!("instead of 'codeatlas mcp serve'.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --service-url <host:port>  Service address to connect to");
    eprintln!(
        "                             Default: 127.0.0.1:52337 (or CODEATLAS_HOST/CODEATLAS_PORT)"
    );
    eprintln!();
    eprintln!("Prerequisites:");
    eprintln!("  The CodeAtlas service must be running ('codeatlas serve').");
    eprintln!();
    eprintln!("Example client configuration (Claude Desktop, Cursor):");
    eprintln!("  {{");
    eprintln!("    \"mcpServers\": {{");
    eprintln!("      \"codeatlas\": {{");
    eprintln!("        \"command\": \"codeatlas\",");
    eprintln!("        \"args\": [\"mcp\", \"bridge\"]");
    eprintln!("      }}");
    eprintln!("    }}");
    eprintln!("  }}");
}

fn print_serve_help() {
    eprintln!("Usage: codeatlas mcp serve [--db <path>]");
    eprintln!();
    eprintln!("Start the MCP tool server over stdio (newline-delimited JSON-RPC).");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --db <path>    Path to the CodeAtlas index database");
    eprintln!("                 Default: ~/.codeatlas/metadata.db");
    eprintln!();
    eprintln!("Repository-scoped tools accept repo_id as a parameter in each tool");
    eprintln!("call. The repo_id is derived from the indexed directory name (e.g.,");
    eprintln!("indexing /home/user/my-app produces repo_id 'my-app').");
    eprintln!();
    eprintln!("All diagnostics are written to stderr. Stdout is reserved for MCP");
    eprintln!("protocol messages only.");
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
    fn parse_missing_db_flag_uses_default() {
        let a: Vec<String> = vec![];
        let opts = parse_serve_args(&a).unwrap();
        // Falls back to the shared store default path.
        assert!(opts.db_path.to_string_lossy().contains("metadata.db"));
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
        assert!(run(&a).is_ok());
    }

    #[test]
    fn mcp_help_flag() {
        let a = args(&["--help"]);
        assert!(run(&a).is_ok());
    }
}
