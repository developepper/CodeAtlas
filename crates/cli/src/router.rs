//! Adapter router for CLI usage.
//!
//! Builds a [`DefaultRouter`] that includes tree-sitter syntax adapters for
//! all supported languages, plus semantic adapters when their runtime
//! dependencies are available.

use std::path::{Path, PathBuf};

use adapter_api::router::DefaultRouter;
use adapter_semantic_typescript::adapter::TypeScriptSemanticAdapter;
use adapter_semantic_typescript::config::TsServerConfig;
use adapter_semantic_typescript::process::TsServerProcess;
use adapter_semantic_typescript::runtime::SemanticRuntime;
use adapter_syntax_treesitter::{create_adapter, supported_languages};
use tracing::{debug, info, warn};

/// Builds the production adapter router for the given repository root.
///
/// Registers:
/// 1. Tree-sitter syntax adapters for all supported languages.
/// 2. TypeScript semantic adapter if `tsserver` can be located and started.
///
/// The returned router is ready for use with [`DefaultRouter::select`] and
/// [`adapter_api::router::default_policy`].
pub fn build_router(source_root: &Path) -> DefaultRouter {
    let mut router = DefaultRouter::new();

    // Register tree-sitter syntax adapters for all supported languages.
    for lang in supported_languages() {
        if let Some(adapter) = create_adapter(lang) {
            router.register(Box::new(adapter));
        }
    }

    // Try to register the TypeScript semantic adapter.
    match try_create_ts_semantic_adapter(source_root) {
        Ok(adapter) => {
            info!("TypeScript semantic adapter registered");
            router.register(adapter);
        }
        Err(reason) => {
            debug!(reason = %reason, "TypeScript semantic adapter not available, using syntax fallback");
        }
    }

    let ids = router.registered_adapter_ids();
    info!(
        adapter_count = ids.len(),
        adapters = ?ids,
        "adapter router initialized"
    );

    router
}

/// Attempts to locate tsserver and create a started TypeScript semantic adapter.
///
/// Returns a boxed adapter ready for registration, or an error message
/// explaining why the adapter could not be created.
fn try_create_ts_semantic_adapter(
    source_root: &Path,
) -> Result<Box<dyn adapter_api::LanguageAdapter>, String> {
    let tsserver_path = find_tsserver(source_root)?;

    let config = TsServerConfig::new(tsserver_path.clone(), source_root.to_path_buf());
    let mut process = TsServerProcess::new(config);

    process.start().map_err(|e| {
        format!(
            "tsserver at '{}' failed to start: {e}",
            tsserver_path.display()
        )
    })?;

    let adapter = TypeScriptSemanticAdapter::new(process);
    Ok(Box::new(adapter))
}

/// Searches for the tsserver binary in order of preference:
///
/// 1. `TSSERVER_PATH` environment variable (explicit override).
/// 2. Local `node_modules/.bin/tsserver` relative to the repository root.
/// 3. System PATH via `which tsserver`.
fn find_tsserver(source_root: &Path) -> Result<PathBuf, String> {
    // 1. Explicit env var override.
    if let Ok(path) = std::env::var("TSSERVER_PATH") {
        let path = PathBuf::from(&path);
        if path.exists() {
            debug!(path = %path.display(), "using tsserver from TSSERVER_PATH");
            return Ok(path);
        }
        warn!(
            path = %path.display(),
            "TSSERVER_PATH is set but does not exist"
        );
    }

    // 2. Local node_modules.
    let local = source_root.join("node_modules/.bin/tsserver");
    if local.exists() {
        debug!(path = %local.display(), "using local tsserver from node_modules");
        return Ok(local);
    }

    // 3. System PATH lookup.
    if let Ok(output) = std::process::Command::new("which").arg("tsserver").output() {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path_str.is_empty() {
                let path = PathBuf::from(&path_str);
                debug!(path = %path.display(), "using tsserver from system PATH");
                return Ok(path);
            }
        }
    }

    Err(
        "tsserver not found (checked TSSERVER_PATH, node_modules/.bin/tsserver, system PATH)"
            .into(),
    )
}
