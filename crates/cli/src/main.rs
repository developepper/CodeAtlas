//! CodeAtlas CLI — local indexing and query commands.

use std::process;

use opentelemetry::trace::TracerProvider;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

mod commands;
mod error;
mod router;

/// Initialises the tracing subscriber stack.
///
/// The base layer is a `tracing-subscriber` `fmt` layer filtered by the
/// `CODEATLAS_LOG` (or `RUST_LOG`) environment variable, defaulting to
/// `info`.
///
/// When `OTEL_EXPORTER_OTLP_ENDPOINT` is set **or** `CODEATLAS_OTEL=1`,
/// an OpenTelemetry span-export layer is added that writes trace spans to
/// stdout in OTLP-JSON format. Replace the stdout exporter with a
/// network exporter (e.g. `opentelemetry-otlp`) for production use.
fn init_tracing() {
    let env_filter = EnvFilter::try_from_env("CODEATLAS_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer().compact();

    let otel_enabled = std::env::var("CODEATLAS_OTEL").is_ok_and(|v| v == "1")
        || std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok();

    if otel_enabled {
        let exporter = opentelemetry_stdout::SpanExporter::default();
        let provider = opentelemetry_sdk::trace::TracerProvider::builder()
            .with_simple_exporter(exporter)
            .build();
        let otel_layer = tracing_opentelemetry::layer().with_tracer(provider.tracer("codeatlas"));

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(otel_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
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
    eprintln!("  help              Show this help message");
}
