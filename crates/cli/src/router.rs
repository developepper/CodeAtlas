//! Adapter router for CLI usage.
//!
//! Builds a [`DefaultRouter`] that includes tree-sitter syntax adapters for
//! all supported languages, plus semantic adapters when their runtime
//! dependencies are available.

use std::path::{Path, PathBuf};

use adapter_api::router::DefaultRouter;
use adapter_semantic_kotlin::adapter::KotlinSemanticAdapter;
use adapter_semantic_kotlin::config::KotlinAnalysisConfig;
use adapter_semantic_kotlin::process::KotlinAnalysisProcess;
use adapter_semantic_kotlin::runtime::KotlinRuntime;
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
/// The returned router is ready for use with [`adapter_api::AdapterRouter::select`] and
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

    // Try to register the Kotlin semantic adapter.
    match try_create_kotlin_semantic_adapter(source_root) {
        Ok(adapter) => {
            info!("Kotlin semantic adapter registered");
            router.register(adapter);
        }
        Err(reason) => {
            debug!(reason = %reason, "Kotlin semantic adapter not available, using syntax fallback");
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

/// Attempts to locate a JVM and the Kotlin analysis bridge JAR, then creates
/// a started Kotlin semantic adapter.
///
/// Returns a boxed adapter ready for registration, or an error message
/// explaining why the adapter could not be created.
fn try_create_kotlin_semantic_adapter(
    source_root: &Path,
) -> Result<Box<dyn adapter_api::LanguageAdapter>, String> {
    let java_path = find_java()?;
    let bridge_jar = find_kotlin_bridge_jar(source_root)?;

    let config = KotlinAnalysisConfig::new(
        java_path.clone(),
        bridge_jar.clone(),
        source_root.to_path_buf(),
    );
    let mut process = KotlinAnalysisProcess::new(config);

    process.start().map_err(|e| {
        format!(
            "Kotlin bridge (java={}, jar={}) failed to start: {e}",
            java_path.display(),
            bridge_jar.display(),
        )
    })?;

    let adapter = KotlinSemanticAdapter::new(process);
    Ok(Box::new(adapter))
}

/// Searches for a `java` binary in order of preference:
///
/// 1. `JAVA_HOME/bin/java` if `JAVA_HOME` is set.
/// 2. System PATH via `which java`.
fn find_java() -> Result<PathBuf, String> {
    // 1. JAVA_HOME
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let path = PathBuf::from(&java_home).join("bin/java");
        if path.exists() {
            debug!(path = %path.display(), "using java from JAVA_HOME");
            return Ok(path);
        }
        warn!(
            java_home = %java_home,
            "JAVA_HOME is set but bin/java does not exist"
        );
    }

    // 2. System PATH lookup.
    if let Ok(output) = std::process::Command::new("which").arg("java").output() {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path_str.is_empty() {
                let path = PathBuf::from(&path_str);
                debug!(path = %path.display(), "using java from system PATH");
                return Ok(path);
            }
        }
    }

    Err("java not found (checked JAVA_HOME, system PATH)".into())
}

/// Searches for the Kotlin analysis bridge JAR in order of preference:
///
/// 1. `KOTLIN_BRIDGE_JAR` environment variable (explicit override).
/// 2. `<source_root>/.codeatlas/kotlin-bridge.jar` (repo-local).
fn find_kotlin_bridge_jar(source_root: &Path) -> Result<PathBuf, String> {
    // 1. Explicit env var.
    if let Ok(path) = std::env::var("KOTLIN_BRIDGE_JAR") {
        let path = PathBuf::from(&path);
        if path.exists() {
            debug!(path = %path.display(), "using Kotlin bridge JAR from KOTLIN_BRIDGE_JAR");
            return Ok(path);
        }
        warn!(
            path = %path.display(),
            "KOTLIN_BRIDGE_JAR is set but does not exist"
        );
    }

    // 2. Repo-local path.
    let local = source_root.join(".codeatlas/kotlin-bridge.jar");
    if local.exists() {
        debug!(path = %local.display(), "using local Kotlin bridge JAR");
        return Ok(local);
    }

    Err(
        "Kotlin bridge JAR not found (checked KOTLIN_BRIDGE_JAR, .codeatlas/kotlin-bridge.jar)"
            .into(),
    )
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
