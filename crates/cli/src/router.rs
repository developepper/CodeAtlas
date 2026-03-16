//! Backend registry builder for CLI usage.
//!
//! Builds a [`DefaultBackendRegistry`] that includes the Rust syntax backend
//! from `syntax-platform`, plus semantic backends when their runtime
//! dependencies are available.

use std::path::{Path, PathBuf};

use core_model::BackendId;
use indexer::registry::DefaultBackendRegistry;
use semantic_kotlin::adapter::KotlinSemanticAdapter;
use semantic_kotlin::config::KotlinAnalysisConfig;
use semantic_kotlin::process::KotlinAnalysisProcess;
use semantic_kotlin::runtime::KotlinRuntime;
use semantic_typescript::adapter::TypeScriptSemanticAdapter;
use semantic_typescript::config::TsServerConfig;
use semantic_typescript::process::TsServerProcess;
use semantic_typescript::runtime::SemanticRuntime;
use syntax_platform::{PhpSyntaxBackend, PythonSyntaxBackend, RustSyntaxBackend};
use tracing::{debug, info, warn};

/// Builds the production backend registry for the given repository root.
///
/// Registers:
/// 1. Rust syntax backend from `syntax-platform`.
/// 2. TypeScript semantic backend if `tsserver` can be located and started.
/// 3. Kotlin semantic backend if `java` and the bridge JAR can be located.
///
/// The returned registry is ready for use with the indexer pipeline.
pub fn build_router(source_root: &Path) -> DefaultBackendRegistry {
    let mut registry = DefaultBackendRegistry::new();

    // Register syntax backends.
    let rust_id = RustSyntaxBackend::backend_id();
    registry.register_syntax(rust_id, Box::new(RustSyntaxBackend::new()));

    let php_id = PhpSyntaxBackend::backend_id();
    registry.register_syntax(php_id, Box::new(PhpSyntaxBackend::new()));

    let python_id = PythonSyntaxBackend::backend_id();
    registry.register_syntax(python_id, Box::new(PythonSyntaxBackend::new()));

    // Try to register the TypeScript semantic backend.
    match try_create_ts_semantic_backend(source_root) {
        Ok((id, backend)) => {
            info!("TypeScript semantic backend registered");
            registry.register_semantic(id, backend);
        }
        Err(reason) => {
            debug!(reason = %reason, "TypeScript semantic backend not available, using syntax fallback");
        }
    }

    // Try to register the Kotlin semantic backend.
    match try_create_kotlin_semantic_backend(source_root) {
        Ok((id, backend)) => {
            info!("Kotlin semantic backend registered");
            registry.register_semantic(id, backend);
        }
        Err(reason) => {
            debug!(reason = %reason, "Kotlin semantic backend not available, using syntax fallback");
        }
    }

    let ids = registry.all_backend_ids();
    info!(
        backend_count = ids.len(),
        backends = ?ids,
        "backend registry initialized"
    );

    registry
}

/// Attempts to locate tsserver and create a started TypeScript semantic backend.
///
/// Returns a backend ID and boxed backend ready for registration, or an error
/// message explaining why the backend could not be created.
fn try_create_ts_semantic_backend(
    source_root: &Path,
) -> Result<(BackendId, Box<dyn semantic_api::SemanticBackend>), String> {
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
    let id = TypeScriptSemanticAdapter::<TsServerProcess>::backend_id();
    Ok((id, Box::new(adapter)))
}

/// Attempts to locate a JVM and the Kotlin analysis bridge JAR, then creates
/// a started Kotlin semantic backend.
///
/// Returns a backend ID and boxed backend ready for registration, or an error
/// message explaining why the backend could not be created.
fn try_create_kotlin_semantic_backend(
    source_root: &Path,
) -> Result<(BackendId, Box<dyn semantic_api::SemanticBackend>), String> {
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
    let id = KotlinSemanticAdapter::<KotlinAnalysisProcess>::backend_id();
    Ok((id, Box::new(adapter)))
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
