//! Tests that tracing spans are emitted for query-engine operations and
//! that query spans properly nest under a caller-provided parent span.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use query_engine::{
    FileOutlineRequest, FileTreeRequest, QueryFilters, QueryService, RepoOutlineRequest,
    StoreQueryService, SymbolQuery, TextQuery,
};
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;

// ---------------------------------------------------------------------------
// Span-capturing layer with parent tracking
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct SpanInfo {
    name: String,
    parent_name: Option<String>,
    fields: BTreeMap<String, String>,
}

#[derive(Default)]
struct FieldCapture {
    fields: BTreeMap<String, String>,
}

impl Visit for FieldCapture {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_string(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }
}

struct SpanTreeCapture {
    spans: Arc<Mutex<Vec<SpanInfo>>>,
}

impl<S> tracing_subscriber::Layer<S> for SpanTreeCapture
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let name = attrs.metadata().name().to_string();
        let mut fields = FieldCapture::default();
        attrs.record(&mut fields);

        let parent_name = if let Some(parent_id) = attrs.parent() {
            ctx.span(parent_id).map(|s| s.name().to_string())
        } else if attrs.is_contextual() {
            ctx.lookup_current().map(|s| s.name().to_string())
        } else {
            None
        };

        self.spans.lock().unwrap().push(SpanInfo {
            name,
            parent_name,
            fields: fields.fields,
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Verifies production query spans are emitted via a real store-backed service.
#[test]
fn store_query_service_emits_spans() {
    let captured = Arc::new(Mutex::new(Vec::<SpanInfo>::new()));
    let layer = SpanTreeCapture {
        spans: Arc::clone(&captured),
    };
    let subscriber = tracing_subscriber::registry().with(layer);

    let db = store::MetadataStore::open_in_memory().unwrap();
    let blob_dir = tempfile::TempDir::new().unwrap();
    let blob_store = store::BlobStore::open(&blob_dir.path().join("blobs")).unwrap();
    let svc = StoreQueryService::new(&db, &blob_store);

    tracing::subscriber::with_default(subscriber, || {
        // These will return errors/empty results but still emit spans.
        let _ = svc.search_symbols(&SymbolQuery {
            repo_id: "r".into(),
            text: "test".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        });
        let _ = svc.get_symbol("nonexistent");
        let _ = svc.get_symbols(&["a", "b"]);
        let _ = svc.get_file_outline(&FileOutlineRequest {
            repo_id: "r".into(),
            file_path: "f.rs".into(),
        });
        let _ = svc.get_file_tree(&FileTreeRequest {
            repo_id: "r".into(),
            path_prefix: None,
        });
        let _ = svc.get_repo_outline(&RepoOutlineRequest {
            repo_id: "r".into(),
        });
        let _ = svc.search_text(&TextQuery {
            repo_id: "r".into(),
            pattern: "test".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        });
    });

    let spans = captured.lock().unwrap();
    let names: Vec<&str> = spans.iter().map(|s| s.name.as_str()).collect();

    let expected = [
        "query_search_symbols",
        "query_get_symbol",
        "query_get_symbols",
        "query_get_file_outline",
        "query_get_file_tree",
        "query_get_repo_outline",
        "query_search_text",
    ];

    for expected_name in &expected {
        assert!(
            names.contains(expected_name),
            "expected '{expected_name}' span, got: {names:?}"
        );
    }
}

/// Verifies that query spans become children of a caller-provided parent,
/// proving context propagation works across the query boundary.
#[test]
fn query_spans_nest_under_caller_parent() {
    let captured = Arc::new(Mutex::new(Vec::<SpanInfo>::new()));
    let layer = SpanTreeCapture {
        spans: Arc::clone(&captured),
    };
    let subscriber = tracing_subscriber::registry().with(layer);

    let db = store::MetadataStore::open_in_memory().unwrap();
    let blob_dir = tempfile::TempDir::new().unwrap();
    let blob_store = store::BlobStore::open(&blob_dir.path().join("blobs")).unwrap();
    let svc = StoreQueryService::new(&db, &blob_store);

    tracing::subscriber::with_default(subscriber, || {
        // Simulate an outer request span (like mcp_tool_call).
        let request_span = tracing::info_span!("simulated_request", tool = "search_symbols");
        let _guard = request_span.enter();

        let _ = svc.search_symbols(&SymbolQuery {
            repo_id: "r".into(),
            text: "test".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        });
    });

    let spans = captured.lock().unwrap();

    // The query span should be a child of the simulated request span.
    let query_span = spans
        .iter()
        .find(|s| s.name == "query_search_symbols")
        .expect("missing query_search_symbols span");

    assert_eq!(
        query_span.parent_name.as_deref(),
        Some("simulated_request"),
        "query_search_symbols should be a child of the caller's span, \
         but parent was {:?}",
        query_span.parent_name
    );
}

/// Verifies that multiple independent query calls each get their own span
/// (no leaking between calls).
#[test]
fn each_query_call_gets_independent_span() {
    let captured = Arc::new(Mutex::new(Vec::<SpanInfo>::new()));
    let layer = SpanTreeCapture {
        spans: Arc::clone(&captured),
    };
    let subscriber = tracing_subscriber::registry().with(layer);

    let db = store::MetadataStore::open_in_memory().unwrap();
    let blob_dir = tempfile::TempDir::new().unwrap();
    let blob_store = store::BlobStore::open(&blob_dir.path().join("blobs")).unwrap();
    let svc = StoreQueryService::new(&db, &blob_store);

    tracing::subscriber::with_default(subscriber, || {
        let _ = svc.get_symbol("id-1");
        let _ = svc.get_symbol("id-2");
        let _ = svc.get_symbol("id-3");
    });

    let spans = captured.lock().unwrap();
    let get_symbol_spans: Vec<_> = spans
        .iter()
        .filter(|s| s.name == "query_get_symbol")
        .collect();

    assert_eq!(
        get_symbol_spans.len(),
        3,
        "expected 3 independent query_get_symbol spans, got {}",
        get_symbol_spans.len()
    );
}

/// Verifies that sensitive query payloads are not attached to span attributes,
/// so OTEL exporters cannot leak raw query content.
#[test]
fn query_spans_do_not_capture_raw_query_text() {
    let captured = Arc::new(Mutex::new(Vec::<SpanInfo>::new()));
    let layer = SpanTreeCapture {
        spans: Arc::clone(&captured),
    };
    let subscriber = tracing_subscriber::registry().with(layer);

    let db = store::MetadataStore::open_in_memory().unwrap();
    let blob_dir = tempfile::TempDir::new().unwrap();
    let blob_store = store::BlobStore::open(&blob_dir.path().join("blobs")).unwrap();
    let svc = StoreQueryService::new(&db, &blob_store);

    tracing::subscriber::with_default(subscriber, || {
        let _ = svc.search_symbols(&SymbolQuery {
            repo_id: "r".into(),
            text: "password_hash".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        });
        let _ = svc.search_text(&TextQuery {
            repo_id: "r".into(),
            pattern: "api_key.*".into(),
            filters: QueryFilters::default(),
            limit: 10,
            offset: 0,
        });
    });

    let spans = captured.lock().unwrap();
    let search_symbols_span = spans
        .iter()
        .find(|s| s.name == "query_search_symbols")
        .expect("missing query_search_symbols span");
    assert_eq!(
        search_symbols_span.fields.get("query_text_redacted"),
        Some(&"true".to_string())
    );
    assert_eq!(
        search_symbols_span.fields.get("query_length"),
        Some(&"13".to_string())
    );
    assert!(
        !search_symbols_span.fields.contains_key("query_text"),
        "raw query text should not be attached to tracing spans"
    );

    let search_text_span = spans
        .iter()
        .find(|s| s.name == "query_search_text")
        .expect("missing query_search_text span");
    assert_eq!(
        search_text_span.fields.get("pattern_redacted"),
        Some(&"true".to_string())
    );
    assert_eq!(
        search_text_span.fields.get("pattern_length"),
        Some(&"9".to_string())
    );
    assert!(
        !search_text_span.fields.contains_key("pattern"),
        "raw search pattern should not be attached to tracing spans"
    );
}
