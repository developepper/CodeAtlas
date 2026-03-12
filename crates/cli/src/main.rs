//! CodeAtlas CLI — local indexing and query commands.

use std::process;

use opentelemetry::trace::TracerProvider;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

mod commands;
mod error;
pub mod logging;
mod router;

/// Initialises the tracing subscriber stack.
///
/// **Log format** is controlled by `CODEATLAS_LOG_FORMAT`:
/// - `json` (default) — structured JSON lines with redaction (spec §13.2).
/// - `compact` — human-readable compact output for local development.
///
/// **Log level** respects `CODEATLAS_LOG` or `RUST_LOG`, defaulting to
/// `info`.
///
/// **OpenTelemetry** span export is enabled when `CODEATLAS_OTEL=1` or
/// `OTEL_EXPORTER_OTLP_ENDPOINT` is set.
fn init_tracing() {
    let env_filter = EnvFilter::try_from_env("CODEATLAS_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let use_json = std::env::var("CODEATLAS_LOG_FORMAT")
        .map(|v| v != "compact")
        .unwrap_or(true);

    let otel_enabled = std::env::var("CODEATLAS_OTEL").is_ok_and(|v| v == "1")
        || std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok();

    if use_json {
        let json_layer = logging::RedactingJsonLayer::new(std::io::stderr);
        let base = tracing_subscriber::registry()
            .with(env_filter)
            .with(json_layer);
        if otel_enabled {
            let exporter = opentelemetry_stdout::SpanExporter::default();
            let provider = opentelemetry_sdk::trace::TracerProvider::builder()
                .with_simple_exporter(exporter)
                .build();
            let otel_layer =
                tracing_opentelemetry::layer().with_tracer(provider.tracer("codeatlas"));
            base.with(otel_layer).init();
        } else {
            base.init();
        }
    } else {
        let fmt_layer = tracing_subscriber::fmt::layer()
            .compact()
            .fmt_fields(logging::RedactingFieldFormatter);
        let base = tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer);
        if otel_enabled {
            let exporter = opentelemetry_stdout::SpanExporter::default();
            let provider = opentelemetry_sdk::trace::TracerProvider::builder()
                .with_simple_exporter(exporter)
                .build();
            let otel_layer =
                tracing_opentelemetry::layer().with_tracer(provider.tracer("codeatlas"));
            base.with(otel_layer).init();
        } else {
            base.init();
        }
    }
}

fn main() {
    init_tracing();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let result = match args[1].as_str() {
        "index" => commands::index::run(&args[2..]),
        "search-symbols" => commands::search_symbols::run(&args[2..]),
        "get-symbol" => commands::get_symbol::run(&args[2..]),
        "file-outline" => commands::file_outline::run(&args[2..]),
        "file-tree" => commands::file_tree::run(&args[2..]),
        "repo-outline" => commands::repo_outline::run(&args[2..]),
        "quality-report" => commands::quality_report::run(&args[2..]),
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        other => {
            eprintln!("error: unknown command '{other}'");
            print_usage();
            process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

fn print_usage() {
    eprintln!("Usage: codeatlas <command> [options]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  index             Index a repository");
    eprintln!("  search-symbols    Search for symbols by name");
    eprintln!("  get-symbol        Get a symbol by ID");
    eprintln!("  file-outline      List symbols in a file");
    eprintln!("  file-tree         List files in a repository");
    eprintln!("  repo-outline      Show repository structure overview");
    eprintln!("  quality-report    Generate quality KPI report");
    eprintln!("  help              Show this help message");
}
