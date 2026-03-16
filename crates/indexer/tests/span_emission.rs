//! Tests that tracing spans are emitted for major pipeline stages and that
//! child stage spans inherit trace context from the parent pipeline span.
//!
//! Uses a custom `tracing` layer to capture span names, parent-child
//! relationships, and field values.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use indexer::{DefaultBackendRegistry, DispatchContext, PipelineContext};
use syntax_platform::RustSyntaxBackend;
use tempfile::TempDir;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;

// ---------------------------------------------------------------------------
// Registry helper
// ---------------------------------------------------------------------------

fn make_registry() -> DefaultBackendRegistry {
    let mut registry = DefaultBackendRegistry::new();
    let rust_backend = RustSyntaxBackend::new();
    let rust_id = RustSyntaxBackend::backend_id();
    registry.register_syntax(rust_id, Box::new(rust_backend));
    registry
}

// ---------------------------------------------------------------------------
// Span-capturing layer with parent tracking
// ---------------------------------------------------------------------------

/// Recorded information about a single span.
#[derive(Clone, Debug)]
struct SpanInfo {
    name: String,
    parent_name: Option<String>,
    fields: Vec<(String, String)>,
}

/// Shared state for the capturing layer.
#[derive(Default)]
struct CaptureState {
    /// All recorded spans in creation order.
    spans: Vec<SpanInfo>,
    /// Map from span ID to name, for resolving parent names.
    id_to_name: HashMap<u64, String>,
}

/// A tracing layer that records span names, parent-child relationships, and
/// field values for every span it observes.
struct SpanTreeCapture {
    state: Arc<Mutex<CaptureState>>,
}

/// Visitor that collects all fields recorded on a span.
struct FieldCollector {
    fields: Vec<(String, String)>,
}

impl tracing::field::Visit for FieldCollector {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.fields
            .push((field.name().to_string(), format!("{value:?}")));
    }
}

impl<S> tracing_subscriber::Layer<S> for SpanTreeCapture
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let name = attrs.metadata().name().to_string();

        // Resolve the parent span name via the registry context.
        let parent_name = if let Some(parent_id) = attrs.parent() {
            // Explicit parent set by the span.
            ctx.span(parent_id).map(|s| s.name().to_string())
        } else if attrs.is_contextual() {
            // Contextual parent: whatever span is currently entered.
            ctx.lookup_current().map(|s| s.name().to_string())
        } else {
            None
        };

        let mut visitor = FieldCollector { fields: Vec::new() };
        attrs.record(&mut visitor);

        let mut state = self.state.lock().unwrap();
        state.id_to_name.insert(id.into_u64(), name.clone());
        state.spans.push(SpanInfo {
            name,
            parent_name,
            fields: visitor.fields,
        });
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn run_pipeline_with_capture(correlation_id: Option<String>) -> Vec<SpanInfo> {
    let state = Arc::new(Mutex::new(CaptureState::default()));
    let layer = SpanTreeCapture {
        state: Arc::clone(&state),
    };
    let subscriber = tracing_subscriber::registry().with(layer);

    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("main.rs"), b"fn main() {}").unwrap();

    let mut db = store::MetadataStore::open_in_memory().unwrap();
    let blob_dir = TempDir::new().unwrap();
    let blob_store = store::BlobStore::open(blob_dir.path()).unwrap();
    let registry = make_registry();

    let ctx = PipelineContext {
        repo_id: "test-repo".into(),
        source_root: dir.path().to_path_buf(),
        registry: &registry,
        dispatch_context: DispatchContext::default(),
        correlation_id,
        use_git_diff: false,
    };

    tracing::subscriber::with_default(subscriber, || {
        indexer::run(&ctx, &mut db, &blob_store).unwrap();
    });

    let state = state.lock().unwrap();
    state.spans.clone()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn pipeline_emits_stage_spans() {
    let spans = run_pipeline_with_capture(Some("test-corr-123".into()));
    let names: Vec<&str> = spans.iter().map(|s| s.name.as_str()).collect();

    assert!(
        names.contains(&"index_pipeline"),
        "expected 'index_pipeline' span, got: {names:?}"
    );
    assert!(
        names.contains(&"stage_discover"),
        "expected 'stage_discover' span, got: {names:?}"
    );
    assert!(
        names.contains(&"stage_extract"),
        "expected 'stage_extract' span, got: {names:?}"
    );
    assert!(
        names.contains(&"stage_persist"),
        "expected 'stage_persist' span, got: {names:?}"
    );
}

#[test]
fn stage_spans_are_children_of_pipeline_span() {
    let spans = run_pipeline_with_capture(Some("ctx-prop-test".into()));

    // Every stage span must have index_pipeline as its parent, proving
    // that span context propagates from the pipeline root into each stage.
    let child_stages = ["stage_discover", "stage_extract", "stage_persist"];

    for stage_name in &child_stages {
        let info = spans
            .iter()
            .find(|s| s.name == *stage_name)
            .unwrap_or_else(|| panic!("missing span '{stage_name}'"));

        assert_eq!(
            info.parent_name.as_deref(),
            Some("index_pipeline"),
            "'{stage_name}' should be a child of 'index_pipeline', but parent was {:?}",
            info.parent_name
        );
    }
}

#[test]
fn pipeline_span_carries_correlation_id() {
    let spans = run_pipeline_with_capture(Some("corr-42".into()));

    let pipeline = spans
        .iter()
        .find(|s| s.name == "index_pipeline")
        .expect("missing index_pipeline span");

    let corr_field = pipeline
        .fields
        .iter()
        .find(|(k, _)| k == "correlation_id")
        .expect("index_pipeline span missing correlation_id field");

    assert!(
        corr_field.1.contains("corr-42"),
        "expected correlation_id='corr-42', got: {:?}",
        corr_field.1
    );
}

#[test]
fn stage_spans_inherit_correlation_context() {
    // Even though child spans don't carry their own correlation_id field,
    // the tracing parent chain means any subscriber/exporter can walk up
    // to index_pipeline and retrieve it. Verify that the parentage holds
    // for a different correlation ID value to avoid false positives.
    let spans = run_pipeline_with_capture(Some("unique-987".into()));

    // Verify the pipeline root has the expected correlation_id.
    let pipeline = spans
        .iter()
        .find(|s| s.name == "index_pipeline")
        .expect("missing index_pipeline span");
    assert!(
        pipeline
            .fields
            .iter()
            .any(|(k, v)| k == "correlation_id" && v.contains("unique-987")),
        "pipeline span should carry correlation_id=unique-987"
    );

    // Verify all stage spans are parented under this pipeline span,
    // so the correlation context is reachable via the span tree.
    for stage in &["stage_discover", "stage_extract", "stage_persist"] {
        let info = spans.iter().find(|s| s.name == *stage).unwrap();
        assert_eq!(
            info.parent_name.as_deref(),
            Some("index_pipeline"),
            "'{stage}' must be parented under index_pipeline for context propagation"
        );
    }
}
