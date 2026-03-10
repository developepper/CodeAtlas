//! CodeAtlas CLI — local indexing and query commands.

use std::process;

mod commands;
mod error;
mod router;

fn main() {
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
