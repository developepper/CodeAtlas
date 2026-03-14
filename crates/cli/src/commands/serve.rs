//! `codeatlas serve` command — start the persistent local HTTP service.

use std::net::IpAddr;
use std::path::PathBuf;

use service::ServiceConfig;

use crate::error::CliError;

struct ServeOpts {
    data_root: Option<PathBuf>,
    port: Option<u16>,
    host: Option<IpAddr>,
}

fn parse_args(args: &[String]) -> Result<ServeOpts, CliError> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Err(CliError::Usage(String::new()));
    }

    let mut data_root: Option<PathBuf> = None;
    let mut port: Option<u16> = None;
    let mut host: Option<IpAddr> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--data-root" => {
                i += 1;
                data_root =
                    Some(PathBuf::from(args.get(i).ok_or_else(|| {
                        CliError::Usage("--data-root requires a value".into())
                    })?));
            }
            "--port" => {
                i += 1;
                let val = args
                    .get(i)
                    .ok_or_else(|| CliError::Usage("--port requires a value".into()))?;
                port = Some(
                    val.parse::<u16>()
                        .map_err(|_| CliError::Usage(format!("invalid port number: {val}")))?,
                );
            }
            "--host" => {
                i += 1;
                let val = args
                    .get(i)
                    .ok_or_else(|| CliError::Usage("--host requires a value".into()))?;
                host = Some(
                    val.parse::<IpAddr>()
                        .map_err(|_| CliError::Usage(format!("invalid host address: {val}")))?,
                );
            }
            other => {
                return Err(CliError::Usage(format!("unknown option: {other}")));
            }
        }
        i += 1;
    }

    Ok(ServeOpts {
        data_root,
        port,
        host,
    })
}

pub fn run(args: &[String]) -> Result<(), CliError> {
    let opts = parse_args(args)?;

    let data_root = match opts.data_root {
        Some(p) => p,
        None => cli::data_root::default_data_root().map_err(CliError::Usage)?,
    };

    // Allow environment variable overrides for port and host.
    let mut config = ServiceConfig::new(data_root);

    if let Some(port) = opts.port {
        config.port = port;
    } else if let Ok(val) = std::env::var("CODEATLAS_PORT") {
        if let Ok(p) = val.parse::<u16>() {
            config.port = p;
        }
    }

    if let Some(host) = opts.host {
        config.host = host;
    } else if let Ok(val) = std::env::var("CODEATLAS_HOST") {
        if let Ok(h) = val.parse::<IpAddr>() {
            config.host = h;
        }
    }

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| CliError::Io(std::io::Error::other(format!("async runtime: {e}"))))?;

    rt.block_on(async { service::run_service(config).await.map_err(CliError::from) })
}

fn print_help() {
    eprintln!("Usage: codeatlas serve [options]");
    eprintln!();
    eprintln!("Start the persistent local HTTP service.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --data-root <path>  Storage root directory");
    eprintln!("                      Default: ~/.codeatlas (or CODEATLAS_DATA_ROOT)");
    eprintln!("  --port <port>       Port to listen on");
    eprintln!("                      Default: 52337 (or CODEATLAS_PORT)");
    eprintln!("  --host <addr>       Bind address");
    eprintln!("                      Default: 127.0.0.1 (or CODEATLAS_HOST)");
    eprintln!();
    eprintln!("The service exposes:");
    eprintln!("  GET  /health              Health check");
    eprintln!("  GET  /status              Service status and metadata");
    eprintln!("  GET  /repos               List all repositories");
    eprintln!("  GET  /repos/<repo_id>     Repository details");
    eprintln!("  DELETE /repos/<repo_id>   Remove a repository");
    eprintln!("  POST /tools/call          Execute a query tool");
}
