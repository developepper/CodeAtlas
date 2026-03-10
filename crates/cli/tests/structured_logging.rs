//! Integration tests for structured JSON logging and redaction enforcement.
//!
//! Verifies that:
//! - Log output consists of valid JSON lines (spec §13.2).
//! - Sensitive fields are redacted per the telemetry policy (spec §12.2).
//! - Non-sensitive fields pass through unmodified.
//! - Span context (including correlation ID) is included in log output.
//! - Compact mode also enforces redaction.

use std::sync::{Arc, Mutex};

use cli::logging::{self, RedactingFieldFormatter, RedactingJsonLayer};
use tracing_subscriber::layer::SubscriberExt;

// ---------------------------------------------------------------------------
// Captured writer for test output
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct CapturedWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl CapturedWriter {
    fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn contents(&self) -> String {
        let buf = self.buffer.lock().expect("lock buffer");
        String::from_utf8_lossy(&buf).to_string()
    }
}

impl std::io::Write for CapturedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.lock().expect("lock").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedWriter {
    type Writer = CapturedWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parses each non-empty line of output as JSON, returning the parsed values.
fn parse_json_lines(output: &str) -> Vec<serde_json::Value> {
    output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("invalid JSON line: {e}\n  line: {line}"))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn output_is_valid_structured_json() {
    let writer = CapturedWriter::new();
    let layer = RedactingJsonLayer::new(writer.clone());
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(files_discovered = 10, "pipeline started");
        tracing::warn!(adapter = "tree-sitter", "adapter failed");
    });

    let output = writer.contents();
    let lines = parse_json_lines(&output);

    assert_eq!(lines.len(), 2, "expected 2 JSON log lines");

    // Verify required top-level keys are present.
    for line in &lines {
        assert!(line.get("timestamp").is_some(), "missing 'timestamp'");
        assert!(line.get("level").is_some(), "missing 'level'");
        assert!(line.get("target").is_some(), "missing 'target'");
        assert!(line.get("message").is_some(), "missing 'message'");
        assert!(line.get("fields").is_some(), "missing 'fields'");
        assert!(line.get("spans").is_some(), "missing 'spans'");
    }

    // Verify specific values.
    assert_eq!(lines[0]["level"], "INFO");
    assert_eq!(lines[0]["message"], "pipeline started");
    assert_eq!(lines[0]["fields"]["files_discovered"], 10);

    assert_eq!(lines[1]["level"], "WARN");
    assert_eq!(lines[1]["message"], "adapter failed");
    assert_eq!(lines[1]["fields"]["adapter"], "tree-sitter");
}

#[test]
fn sensitive_fields_are_redacted_in_events() {
    let writer = CapturedWriter::new();
    let layer = RedactingJsonLayer::new(writer.clone());
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(
            source_root = "/home/user/secret-project",
            query_text = "password_hash",
            pattern = "api_key.*",
            repo_id = "my-repo",
            "search started"
        );
    });

    let output = writer.contents();
    let lines = parse_json_lines(&output);
    assert_eq!(lines.len(), 1);

    let fields = &lines[0]["fields"];

    // Sensitive fields must be redacted.
    assert_eq!(
        fields["source_root"], "[REDACTED]",
        "source_root should be redacted"
    );
    assert_eq!(
        fields["query_text"], "[REDACTED]",
        "query_text should be redacted"
    );
    assert_eq!(
        fields["pattern"], "[REDACTED]",
        "pattern should be redacted"
    );

    // Non-sensitive fields must pass through.
    assert_eq!(
        fields["repo_id"], "my-repo",
        "repo_id should not be redacted"
    );

    // Redacted values must NOT appear anywhere in the raw output.
    assert!(
        !output.contains("/home/user/secret-project"),
        "absolute path leaked into log output"
    );
    assert!(
        !output.contains("password_hash"),
        "query text leaked into log output"
    );
    assert!(
        !output.contains("api_key"),
        "search pattern leaked into log output"
    );
}

#[test]
fn sensitive_fields_are_redacted_in_spans() {
    let writer = CapturedWriter::new();
    let layer = RedactingJsonLayer::new(writer.clone());
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!(
            "index_pipeline",
            source_root = "/secret/path",
            repo_id = "visible-repo",
        );
        let _guard = span.enter();
        tracing::info!("pipeline started");
    });

    let output = writer.contents();
    let lines = parse_json_lines(&output);
    assert_eq!(lines.len(), 1);

    // Span fields should be in the spans array.
    let spans = lines[0]["spans"].as_array().expect("spans should be array");
    assert!(!spans.is_empty(), "should have at least one span");

    let pipeline_span = &spans[0];
    assert_eq!(pipeline_span["name"], "index_pipeline");
    assert_eq!(
        pipeline_span["source_root"], "[REDACTED]",
        "source_root in span should be redacted"
    );
    assert_eq!(
        pipeline_span["repo_id"], "visible-repo",
        "repo_id in span should not be redacted"
    );

    // Verify the raw value doesn't leak.
    assert!(
        !output.contains("/secret/path"),
        "absolute path leaked through span fields"
    );
}

#[test]
fn span_context_propagates_to_events() {
    let writer = CapturedWriter::new();
    let layer = RedactingJsonLayer::new(writer.clone());
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let outer = tracing::info_span!("request", correlation_id = "job-42");
        let _outer_guard = outer.enter();

        let inner = tracing::info_span!("stage_discover", repo_id = "test");
        let _inner_guard = inner.enter();

        tracing::info!(files_discovered = 5, "discovery complete");
    });

    let output = writer.contents();
    let lines = parse_json_lines(&output);
    assert_eq!(lines.len(), 1);

    let spans = lines[0]["spans"].as_array().expect("spans array");
    assert_eq!(spans.len(), 2, "should have 2 spans (outer + inner)");

    // Root → leaf ordering.
    assert_eq!(spans[0]["name"], "request");
    assert_eq!(spans[0]["correlation_id"], "job-42");

    assert_eq!(spans[1]["name"], "stage_discover");
    assert_eq!(spans[1]["repo_id"], "test");
}

#[test]
fn root_field_is_redacted() {
    let writer = CapturedWriter::new();
    let layer = RedactingJsonLayer::new(writer.clone());
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(root = "/home/user/repos/project", "discovery started");
    });

    let output = writer.contents();
    let lines = parse_json_lines(&output);
    assert_eq!(lines[0]["fields"]["root"], "[REDACTED]");
    assert!(
        !output.contains("/home/user/repos/project"),
        "absolute root path leaked"
    );
}

#[test]
fn non_sensitive_metrics_pass_through() {
    let writer = CapturedWriter::new();
    let layer = RedactingJsonLayer::new(writer.clone());
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(
            files_discovered = 42_u64,
            files_parsed = 40_u64,
            files_errored = 2_u64,
            symbols_extracted = 300_u64,
            "pipeline complete"
        );
    });

    let output = writer.contents();
    let lines = parse_json_lines(&output);
    let fields = &lines[0]["fields"];

    assert_eq!(fields["files_discovered"], 42);
    assert_eq!(fields["files_parsed"], 40);
    assert_eq!(fields["files_errored"], 2);
    assert_eq!(fields["symbols_extracted"], 300);
}

#[test]
fn redaction_policy_is_complete() {
    // Cross-reference: all fields identified as sensitive in the spec must
    // be in the SENSITIVE_FIELDS list.
    let required_sensitive = ["source_root", "root", "query_text", "pattern"];

    for field in &required_sensitive {
        assert!(
            logging::is_sensitive(field),
            "field '{field}' should be in the redaction policy"
        );
    }
}

// ---------------------------------------------------------------------------
// Compact mode redaction tests
// ---------------------------------------------------------------------------

#[test]
fn compact_mode_redacts_sensitive_event_fields() {
    let writer = CapturedWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .fmt_fields(RedactingFieldFormatter)
        .with_writer(writer.clone())
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(
            source_root = "/home/user/secret-project",
            query_text = "password_hash",
            repo_id = "my-repo",
            "search started"
        );
    });

    let output = writer.contents();

    // Sensitive values must not appear in compact output.
    assert!(
        !output.contains("/home/user/secret-project"),
        "absolute path leaked in compact mode: {output}"
    );
    assert!(
        !output.contains("password_hash"),
        "query text leaked in compact mode: {output}"
    );

    // Field names should still be present (with redacted values).
    assert!(
        output.contains("source_root=[REDACTED]"),
        "expected source_root=[REDACTED] in compact output: {output}"
    );
    assert!(
        output.contains("query_text=[REDACTED]"),
        "expected query_text=[REDACTED] in compact output: {output}"
    );

    // Non-sensitive fields should pass through.
    assert!(
        output.contains("repo_id=my-repo"),
        "expected repo_id=my-repo in compact output: {output}"
    );
}

#[test]
fn compact_mode_redacts_sensitive_span_fields() {
    let writer = CapturedWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .fmt_fields(RedactingFieldFormatter)
        .with_writer(writer.clone())
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!(
            "index_pipeline",
            source_root = "/secret/path",
            repo_id = "visible-repo",
        );
        let _guard = span.enter();
        tracing::info!("pipeline started");
    });

    let output = writer.contents();

    // The absolute path must not appear anywhere.
    assert!(
        !output.contains("/secret/path"),
        "absolute path leaked through compact span fields: {output}"
    );

    // The redacted placeholder should appear instead.
    assert!(
        output.contains("[REDACTED]"),
        "expected [REDACTED] in compact span output: {output}"
    );

    // Non-sensitive span fields should pass through.
    assert!(
        output.contains("visible-repo"),
        "expected repo_id to pass through in compact mode: {output}"
    );
}

#[test]
fn compact_mode_passes_through_non_sensitive_fields() {
    let writer = CapturedWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .fmt_fields(RedactingFieldFormatter)
        .with_writer(writer.clone())
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(
            files_discovered = 42_u64,
            adapter = "tree-sitter",
            "stage complete"
        );
    });

    let output = writer.contents();

    assert!(
        output.contains("files_discovered=42"),
        "expected files_discovered=42 in compact output: {output}"
    );
    assert!(
        output.contains("adapter="),
        "expected adapter field in compact output: {output}"
    );
    assert!(
        !output.contains("[REDACTED]"),
        "non-sensitive output should not contain [REDACTED]: {output}"
    );
}
