//! Integration tests for the Kotlin semantic adapter with the real runtime.
//!
//! These tests exercise `KotlinSemanticAdapter<KotlinAnalysisProcess>` — the
//! concrete adapter+runtime wiring that will run in production. This is the
//! acceptance-level test for issue #66: "adapter runtime + baseline extraction
//! works on fixture project."
//!
//! ## How it works
//!
//! A lightweight mock bridge script (written in Python) is created at test
//! time and speaks the same Content-Length framed JSON protocol as the real
//! JVM bridge. The `KotlinAnalysisProcess` spawns it as a real subprocess,
//! exercising the full lifecycle: spawn → reader thread → ping handshake →
//! analyze request → response parsing → shutdown.
//!
//! ## Test tiers
//!
//! 1. **Always-run tests** use the mock bridge script to exercise the full
//!    `KotlinAnalysisProcess` lifecycle against a real on-disk fixture project.
//!    No Java or bridge JAR required.
//!
//! 2. **`#[ignore]` tests** require a working Java installation and the real
//!    Kotlin analysis bridge JAR. Run them with:
//!    ```sh
//!    KOTLIN_BRIDGE_JAR=/path/to/bridge.jar cargo test -p adapter-semantic-kotlin --ignored
//!    ```

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use adapter_api::{IndexContext, LanguageAdapter, SourceFile};
use adapter_semantic_kotlin::adapter::KotlinSemanticAdapter;
use adapter_semantic_kotlin::config::KotlinAnalysisConfig;
use adapter_semantic_kotlin::process::KotlinAnalysisProcess;
use adapter_semantic_kotlin::runtime::KotlinRuntime;
use core_model::SymbolKind;
use tempfile::TempDir;

/// Fixture Kotlin source used for integration testing.
const KOTLIN_FIXTURE: &str = r#"/** Configuration for processing. */
data class Config(
    val name: String,
    val limit: Int
)

/** Creates a new config with defaults. */
fun create(name: String): Config {
    return Config(name, 100)
}

class Processor {
    /** Processes the config. */
    fun process(config: Config): Boolean {
        return config.limit > 0
    }
}

/** Operating mode. */
enum class Mode {
    Fast,
    Precise
}

const val MAX_SIZE: Int = 1024
"#;

/// A mock bridge script that speaks the Content-Length framed JSON protocol.
///
/// It reads requests from stdin, parses the `command` field, and responds:
/// - `ping` → success response (handshake for readiness probe)
/// - `analyze` → success response with canned analysis body matching the fixture
/// - `shutdown` → exits cleanly
///
/// This is intentionally a real subprocess so that `KotlinAnalysisProcess`
/// exercises its full lifecycle: spawn, reader thread, stdin/stdout I/O,
/// Content-Length framing, and graceful shutdown.
const MOCK_BRIDGE_SCRIPT: &str = r#"#!/usr/bin/env python3
"""Mock Kotlin analysis bridge for integration testing."""
import json
import sys

FIXTURE_BODY = [
    {
        "name": "Config",
        "kind": "class",
        "modifiers": "data",
        "signature": "data class Config(val name: String, val limit: Int)",
        "startLine": 2,
        "endLine": 5,
        "startByte": 37,
        "byteLength": 54,
        "childItems": []
    },
    {
        "name": "create",
        "kind": "fun",
        "modifiers": "",
        "signature": "fun create(name: String): Config",
        "startLine": 8,
        "endLine": 10,
        "startByte": 133,
        "byteLength": 64,
        "childItems": []
    },
    {
        "name": "Processor",
        "kind": "class",
        "modifiers": "",
        "startLine": 12,
        "endLine": 17,
        "startByte": 199,
        "byteLength": 107,
        "childItems": [
            {
                "name": "process",
                "kind": "fun",
                "modifiers": "",
                "signature": "fun process(config: Config): Boolean",
                "startLine": 14,
                "endLine": 16,
                "startByte": 249,
                "byteLength": 55,
                "childItems": []
            }
        ]
    },
    {
        "name": "Mode",
        "kind": "enum",
        "modifiers": "",
        "signature": "enum class Mode",
        "startLine": 20,
        "endLine": 23,
        "startByte": 330,
        "byteLength": 51,
        "childItems": [
            {
                "name": "Fast",
                "kind": "enum_entry",
                "modifiers": "",
                "startLine": 21,
                "endLine": 21,
                "startByte": 350,
                "byteLength": 4,
                "childItems": []
            },
            {
                "name": "Precise",
                "kind": "enum_entry",
                "modifiers": "",
                "startLine": 22,
                "endLine": 22,
                "startByte": 360,
                "byteLength": 7,
                "childItems": []
            }
        ]
    },
    {
        "name": "MAX_SIZE",
        "kind": "const",
        "modifiers": "",
        "signature": "const val MAX_SIZE: Int = 1024",
        "startLine": 25,
        "endLine": 25,
        "startByte": 383,
        "byteLength": 30,
        "childItems": []
    }
]


def read_request():
    """Read a Content-Length framed JSON request from stdin."""
    while True:
        line = sys.stdin.readline()
        if not line:
            return None
        line = line.strip()
        if not line:
            continue
        if line.startswith("Content-Length: "):
            length = int(line.split(": ", 1)[1])
            sys.stdin.readline()  # consume \r\n separator
            body = sys.stdin.read(length)
            return json.loads(body)
        if line.startswith("{"):
            return json.loads(line)


def send_response(response):
    """Write a Content-Length framed JSON response to stdout."""
    body = json.dumps(response, separators=(",", ":"))
    header = f"Content-Length: {len(body)}\r\n\r\n"
    sys.stdout.write(header)
    sys.stdout.write(body)
    sys.stdout.flush()


def main():
    while True:
        request = read_request()
        if request is None:
            break

        command = request.get("command", "")
        seq = request.get("seq", 0)

        if command == "shutdown":
            send_response({
                "seq": 0,
                "type": "response",
                "command": "shutdown",
                "request_seq": seq,
                "success": True
            })
            break
        elif command == "ping":
            send_response({
                "seq": 0,
                "type": "response",
                "command": "ping",
                "request_seq": seq,
                "success": True
            })
        elif command == "analyze":
            send_response({
                "seq": 0,
                "type": "response",
                "command": "analyze",
                "request_seq": seq,
                "success": True,
                "body": FIXTURE_BODY
            })
        else:
            send_response({
                "seq": 0,
                "type": "response",
                "command": command,
                "request_seq": seq,
                "success": True
            })


if __name__ == "__main__":
    main()
"#;

/// A thin shell wrapper that acts as a fake `java` binary.
///
/// It strips the `-jar` flag and runs the remaining argument (the bridge
/// script) with `python3`. This lets `build_command()` produce
/// `fake_java -jar mock_bridge.py` which becomes `python3 mock_bridge.py`.
///
/// Using a wrapper avoids ETXTBSY errors on Linux — the kernel only
/// returns that error when exec'ing a file that is still being written,
/// and here we exec `/bin/sh` (the wrapper) which delegates to `python3`.
const FAKE_JAVA_WRAPPER: &str = r#"#!/bin/sh
# Fake java: find the argument after -jar and run it with python3.
while [ $# -gt 0 ]; do
    case "$1" in
        -jar) shift; exec python3 "$1" ;;
        *) shift ;;
    esac
done
exit 1
"#;

/// Helper: creates a temp directory with a Kotlin fixture file and mock bridge.
struct FixtureProject {
    _tempdir: TempDir,
    root: PathBuf,
    bridge_script: PathBuf,
    fake_java: PathBuf,
}

impl FixtureProject {
    fn new() -> Self {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let root = tempdir.path().to_path_buf();

        // Write fixture source file.
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(src_dir.join("Config.kt"), KOTLIN_FIXTURE).expect("write fixture");

        // Write mock bridge script.
        let bridge_script = root.join("mock_bridge.py");
        fs::write(&bridge_script, MOCK_BRIDGE_SCRIPT).expect("write mock bridge");

        // Write fake java wrapper.
        // Use explicit open → write → sync → close to ensure the kernel
        // sees the file as fully written before we exec it. Without the
        // sync + explicit drop, the kernel may return ETXTBSY on exec
        // (especially in CI containers using overlayfs/tmpfs).
        let fake_java = root.join("fake_java.sh");
        {
            use std::io::Write;
            let mut f = fs::File::create(&fake_java).expect("create fake java");
            f.write_all(FAKE_JAVA_WRAPPER.as_bytes())
                .expect("write fake java");
            f.sync_all().expect("sync fake java");
            // f is dropped here, closing the fd before chmod + exec.
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&fake_java, fs::Permissions::from_mode(0o755))
                .expect("make fake_java executable");
        }

        Self {
            _tempdir: tempdir,
            root,
            bridge_script,
            fake_java,
        }
    }

    fn make_context(&self) -> IndexContext {
        IndexContext {
            repo_id: "fixture-project".to_string(),
            source_root: self.root.clone(),
        }
    }

    fn make_source_file(&self) -> SourceFile {
        let rel = PathBuf::from("src/Config.kt");
        let abs = self.root.join(&rel);
        let content = fs::read(&abs).expect("read fixture");
        SourceFile {
            relative_path: rel,
            absolute_path: abs,
            content,
            language: "kotlin".to_string(),
        }
    }

    /// Creates a `KotlinAnalysisProcess` using the mock bridge script.
    ///
    /// The mock bridge is a Python script that speaks the same Content-Length
    /// framed JSON protocol as the real JVM bridge. `java_path` is set to a
    /// thin shell wrapper (`fake_java.sh`) that strips `-jar` and runs the
    /// bridge script with `python3`.
    ///
    /// Using a wrapper (rather than exec'ing the Python script directly)
    /// avoids ETXTBSY errors on Linux when parallel tests write and execute
    /// the script simultaneously.
    fn make_mock_process(&self) -> KotlinAnalysisProcess {
        let config = KotlinAnalysisConfig::new(
            self.fake_java.clone(),
            self.bridge_script.clone(),
            self.root.clone(),
        )
        .with_init_timeout(Duration::from_secs(10))
        .with_request_timeout(Duration::from_secs(5));
        KotlinAnalysisProcess::new(config)
    }
}

/// Helper: creates a `KotlinAnalysisProcess` with paths that don't exist,
/// for testing error handling through the real runtime type.
fn make_unavailable_process(working_dir: &std::path::Path) -> KotlinAnalysisProcess {
    let config = KotlinAnalysisConfig::new(
        PathBuf::from("/nonexistent/java"),
        PathBuf::from("/nonexistent/bridge.jar"),
        working_dir.to_path_buf(),
    );
    KotlinAnalysisProcess::new(config)
}

/// Helper: creates a `KotlinAnalysisProcess` using env vars for Java + bridge JAR.
/// Returns `None` if the required env var `KOTLIN_BRIDGE_JAR` is not set.
fn make_real_process(working_dir: &std::path::Path) -> Option<KotlinAnalysisProcess> {
    let jar_path = std::env::var("KOTLIN_BRIDGE_JAR").ok()?;
    let java_path = std::env::var("JAVA_PATH").unwrap_or_else(|_| "java".to_string());
    let config = KotlinAnalysisConfig::new(
        PathBuf::from(java_path),
        PathBuf::from(jar_path),
        working_dir.to_path_buf(),
    );
    Some(KotlinAnalysisProcess::new(config))
}

// ---------------------------------------------------------------------------
// Tier 1: Always-run tests — mock bridge subprocess
// ---------------------------------------------------------------------------

#[test]
fn mock_bridge_starts_and_becomes_ready() {
    let project = FixtureProject::new();
    let mut process = project.make_mock_process();
    process
        .start()
        .expect("mock bridge should start and pass ping handshake");
    assert!(process.is_healthy());
}

#[test]
fn mock_bridge_extracts_expected_symbols_from_fixture() {
    let project = FixtureProject::new();
    let mut process = project.make_mock_process();
    process.start().expect("mock bridge should start");

    let adapter = KotlinSemanticAdapter::new(process);
    let ctx = project.make_context();
    let file = project.make_source_file();
    let output = adapter
        .index_file(&ctx, &file)
        .expect("index_file must succeed with mock bridge");

    // Verify provenance
    assert_eq!(output.source_adapter, "semantic-kotlin-v1");
    assert_eq!(output.quality_level, core_model::QualityLevel::Semantic);

    // Verify expected symbols are present
    let names: Vec<&str> = output.symbols.iter().map(|s| s.name.as_str()).collect();
    for expected in &["Config", "create", "process", "Mode", "MAX_SIZE"] {
        assert!(
            names.contains(expected),
            "expected symbol '{expected}' not found in: {names:?}"
        );
    }

    assert!(
        output.symbols.len() >= 5,
        "expected at least 5 symbols, got {}",
        output.symbols.len()
    );
}

#[test]
fn mock_bridge_symbol_kinds_are_correct() {
    let project = FixtureProject::new();
    let mut process = project.make_mock_process();
    process.start().expect("mock bridge should start");

    let adapter = KotlinSemanticAdapter::new(process);
    let ctx = project.make_context();
    let file = project.make_source_file();
    let output = adapter.index_file(&ctx, &file).unwrap();

    let find = |name: &str| {
        output
            .symbols
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| {
                let names: Vec<&str> = output.symbols.iter().map(|s| s.name.as_str()).collect();
                panic!("symbol '{name}' not found in: {names:?}")
            })
    };

    assert_eq!(find("Config").kind, SymbolKind::Class, "data class → Class");
    assert_eq!(find("create").kind, SymbolKind::Function);
    assert_eq!(find("Processor").kind, SymbolKind::Class);
    assert_eq!(find("process").kind, SymbolKind::Method);
    assert_eq!(find("Mode").kind, SymbolKind::Type, "enum → Type");
    assert_eq!(find("MAX_SIZE").kind, SymbolKind::Constant);
}

#[test]
fn mock_bridge_qualified_names_are_canonical() {
    let project = FixtureProject::new();
    let mut process = project.make_mock_process();
    process.start().expect("mock bridge should start");

    let adapter = KotlinSemanticAdapter::new(process);
    let ctx = project.make_context();
    let file = project.make_source_file();
    let output = adapter.index_file(&ctx, &file).unwrap();

    let find = |name: &str| {
        output
            .symbols
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("symbol '{name}' not found"))
    };

    assert_eq!(find("Config").qualified_name, "Config");
    assert!(find("Config").parent_qualified_name.is_none());

    assert_eq!(find("create").qualified_name, "create");
    assert!(find("create").parent_qualified_name.is_none());

    assert_eq!(find("process").qualified_name, "Processor::process");
    assert_eq!(
        find("process").parent_qualified_name.as_deref(),
        Some("Processor")
    );

    assert_eq!(find("Mode").qualified_name, "Mode");
    assert_eq!(find("MAX_SIZE").qualified_name, "MAX_SIZE");
}

#[test]
fn mock_bridge_symbol_ids_match_canonical_form() {
    let project = FixtureProject::new();
    let mut process = project.make_mock_process();
    process.start().expect("mock bridge should start");

    let adapter = KotlinSemanticAdapter::new(process);
    let ctx = project.make_context();
    let file = project.make_source_file();
    let output = adapter.index_file(&ctx, &file).unwrap();
    let file_path = "src/Config.kt";

    let find = |name: &str| {
        output
            .symbols
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("symbol '{name}' not found"))
    };

    let expected: &[(&str, &str)] = &[
        ("Config", "src/Config.kt::Config#class"),
        ("create", "src/Config.kt::create#function"),
        ("Processor", "src/Config.kt::Processor#class"),
        ("process", "src/Config.kt::Processor::process#method"),
        ("Mode", "src/Config.kt::Mode#type"),
        ("MAX_SIZE", "src/Config.kt::MAX_SIZE#constant"),
    ];

    for (name, expected_id) in expected {
        let sym = find(name);
        let actual_id =
            core_model::symbol_id::build_symbol_id(file_path, &sym.qualified_name, sym.kind)
                .unwrap_or_else(|e| panic!("symbol '{name}' failed ID construction: {e}"));
        assert_eq!(
            &actual_id, expected_id,
            "symbol '{name}' produced wrong canonical ID"
        );

        core_model::symbol_id::validate_symbol_id(&actual_id)
            .unwrap_or_else(|e| panic!("symbol '{name}' ID '{actual_id}' failed validation: {e}"));
    }
}

#[test]
fn mock_bridge_confidence_metadata_present() {
    let project = FixtureProject::new();
    let mut process = project.make_mock_process();
    process.start().expect("mock bridge should start");

    let adapter = KotlinSemanticAdapter::new(process);
    let ctx = project.make_context();
    let file = project.make_source_file();
    let output = adapter.index_file(&ctx, &file).unwrap();

    for sym in &output.symbols {
        let score = sym
            .confidence_score
            .unwrap_or_else(|| panic!("symbol '{}' missing confidence", sym.name));
        assert!(
            (0.0..=1.0).contains(&score),
            "symbol '{}' confidence {score} out of range",
            sym.name
        );
    }
}

#[test]
fn mock_bridge_extraction_is_deterministic() {
    let project = FixtureProject::new();
    let mut process = project.make_mock_process();
    process.start().expect("mock bridge should start");

    let adapter = KotlinSemanticAdapter::new(process);
    let ctx = project.make_context();
    let file = project.make_source_file();

    let out1 = adapter.index_file(&ctx, &file).unwrap();
    let out2 = adapter.index_file(&ctx, &file).unwrap();

    assert_eq!(out1.symbols.len(), out2.symbols.len());
    for (a, b) in out1.symbols.iter().zip(out2.symbols.iter()) {
        assert_eq!(a.name, b.name, "determinism: name differs");
        assert_eq!(a.kind, b.kind, "determinism: kind differs for '{}'", a.name);
        assert_eq!(a.span, b.span, "determinism: span differs for '{}'", a.name);
        assert_eq!(
            a.qualified_name, b.qualified_name,
            "determinism: qualified_name differs for '{}'",
            a.name
        );
        assert_eq!(
            a.confidence_score, b.confidence_score,
            "determinism: confidence differs for '{}'",
            a.name
        );
    }
}

// -- Error path tests with real runtime type --

#[test]
fn real_runtime_type_wires_up_with_adapter() {
    let project = FixtureProject::new();
    let process = make_unavailable_process(&project.root);
    let adapter = KotlinSemanticAdapter::new(process);

    assert_eq!(adapter.adapter_id(), "semantic-kotlin-v1");
    assert_eq!(adapter.language(), "kotlin");

    let caps = adapter.capabilities();
    assert_eq!(caps.quality_level, core_model::QualityLevel::Semantic);
    assert!(caps.supports_type_refs);
    assert!(caps.supports_call_refs);
}

#[test]
fn real_runtime_rejects_unsupported_language() {
    let project = FixtureProject::new();
    let process = make_unavailable_process(&project.root);
    let adapter = KotlinSemanticAdapter::new(process);

    let ctx = project.make_context();
    let file = SourceFile {
        language: "python".to_string(),
        ..project.make_source_file()
    };
    let err = adapter
        .index_file(&ctx, &file)
        .expect_err("should reject unsupported language");
    assert!(err.to_string().contains("unsupported language"));
}

#[test]
fn real_runtime_empty_file_short_circuits() {
    let project = FixtureProject::new();
    let process = make_unavailable_process(&project.root);
    let adapter = KotlinSemanticAdapter::new(process);

    let ctx = project.make_context();
    let file = SourceFile {
        content: Vec::new(),
        ..project.make_source_file()
    };
    let output = adapter.index_file(&ctx, &file).unwrap();
    assert!(output.symbols.is_empty());
    assert_eq!(output.source_adapter, "semantic-kotlin-v1");
    assert_eq!(output.quality_level, core_model::QualityLevel::Semantic);
}

#[test]
fn real_runtime_propagates_spawn_failure_as_adapter_error() {
    let project = FixtureProject::new();
    let process = make_unavailable_process(&project.root);
    let adapter = KotlinSemanticAdapter::new(process);

    let ctx = project.make_context();
    let file = project.make_source_file();
    let err = adapter
        .index_file(&ctx, &file)
        .expect_err("should fail with unavailable runtime");

    let msg = err.to_string();
    assert!(!msg.is_empty(), "error message must be non-empty");
}

#[test]
fn fixture_project_has_real_kotlin_source_on_disk() {
    let project = FixtureProject::new();
    let file_path = project.root.join("src/Config.kt");

    assert!(file_path.exists(), "fixture file must exist on disk");
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("data class Config"));
    assert!(content.contains("fun create"));
    assert!(content.contains("class Processor"));
    assert!(content.contains("enum class Mode"));
    assert!(content.contains("const val MAX_SIZE"));
}

// ---------------------------------------------------------------------------
// Tier 2: Full end-to-end tests (require Java + real bridge JAR)
// ---------------------------------------------------------------------------
//
// Run with: KOTLIN_BRIDGE_JAR=/path/to/bridge.jar cargo test -p adapter-semantic-kotlin --ignored

#[test]
#[ignore]
fn e2e_fixture_project_extracts_expected_symbols() {
    let project = FixtureProject::new();
    let mut process = make_real_process(&project.root)
        .expect("KOTLIN_BRIDGE_JAR env var must be set for e2e tests");
    process.start().expect("bridge must start for e2e test");

    let adapter = KotlinSemanticAdapter::new(process);
    let ctx = project.make_context();
    let file = project.make_source_file();
    let output = adapter
        .index_file(&ctx, &file)
        .expect("index_file must succeed with real runtime");

    assert_eq!(output.source_adapter, "semantic-kotlin-v1");
    assert_eq!(output.quality_level, core_model::QualityLevel::Semantic);

    let names: Vec<&str> = output.symbols.iter().map(|s| s.name.as_str()).collect();
    for expected in &["Config", "create", "process", "Mode", "MAX_SIZE"] {
        assert!(
            names.contains(expected),
            "expected symbol '{expected}' not found in: {names:?}"
        );
    }

    assert!(
        output.symbols.len() >= 5,
        "expected at least 5 symbols, got {}",
        output.symbols.len()
    );
}

#[test]
#[ignore]
fn e2e_fixture_project_symbol_ids_match_canonical_form() {
    let project = FixtureProject::new();
    let mut process = make_real_process(&project.root)
        .expect("KOTLIN_BRIDGE_JAR env var must be set for e2e tests");
    process.start().expect("bridge must start");

    let adapter = KotlinSemanticAdapter::new(process);
    let ctx = project.make_context();
    let file = project.make_source_file();
    let output = adapter.index_file(&ctx, &file).unwrap();
    let file_path = "src/Config.kt";

    let find = |name: &str| {
        output
            .symbols
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("symbol '{name}' not found"))
    };

    let expected: &[(&str, &str)] = &[
        ("Config", "src/Config.kt::Config#class"),
        ("create", "src/Config.kt::create#function"),
        ("Processor", "src/Config.kt::Processor#class"),
        ("process", "src/Config.kt::Processor::process#method"),
        ("Mode", "src/Config.kt::Mode#type"),
        ("MAX_SIZE", "src/Config.kt::MAX_SIZE#constant"),
    ];

    for (name, expected_id) in expected {
        let sym = find(name);
        let actual_id =
            core_model::symbol_id::build_symbol_id(file_path, &sym.qualified_name, sym.kind)
                .unwrap_or_else(|e| panic!("symbol '{name}' failed ID construction: {e}"));
        assert_eq!(
            &actual_id, expected_id,
            "symbol '{name}' produced wrong canonical ID"
        );
    }
}
