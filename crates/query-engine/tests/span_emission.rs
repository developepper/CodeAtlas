//! Tests that tracing spans are emitted for query-engine operations and
//! that query spans properly nest under a caller-provided parent span.

use std::sync::{Arc, Mutex};

use query_engine::{
    FileOutlineRequest, FileTreeRequest, QueryFilters, QueryService, RepoOutlineRequest,
    StoreQueryService, SymbolQuery, TextQuery,
};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;

// ---------------------------------------------------------------------------
// Span-capturing layer with parent tracking
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct SpanInfo {
    name: String,
    parent_name: Option<String>,
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

        let parent_name = if let Some(parent_id) = attrs.parent() {
            ctx.span(parent_id).map(|s| s.name().to_string())
        } else if attrs.is_contextual() {
            ctx.lookup_current().map(|s| s.name().to_string())
        } else {
            None
        };

        self.spans
            .lock()
            .unwrap()
            .push(SpanInfo { name, parent_name });
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
    let svc = StoreQueryService::new(&db);

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
    let svc = StoreQueryService::new(&db);

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
    let svc = StoreQueryService::new(&db);

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
