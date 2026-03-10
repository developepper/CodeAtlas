//! Structured logging with redaction policy enforcement.
//!
//! Provides two redaction-aware output modes:
//!
//! - [`RedactingJsonLayer`] — structured JSON lines (spec §13.2).
//! - [`RedactingFieldFormatter`] — compact human-readable output for local
//!   development, plugged into `tracing_subscriber::fmt` via `.fmt_fields()`.
//!
//! Both modes replace sensitive field values with `[REDACTED]` per the
//! telemetry policy (spec §12.2, §13.2).
//!
//! # Redaction policy
//!
//! Fields listed in [`SENSITIVE_FIELDS`] have their values fully replaced.
//! The field *name* is preserved so log consumers can see that the field
//! was present but its value was suppressed. Relative file paths and
//! non-secret metadata pass through unredacted.

use std::collections::BTreeMap;
use std::fmt;
use std::io;

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tracing::field::{Field, Visit};
use tracing::Subscriber;
use tracing_subscriber::field::RecordFields;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::FormatFields;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

// ---------------------------------------------------------------------------
// Redaction policy
// ---------------------------------------------------------------------------

/// Field names whose values are replaced with `[REDACTED]` in log output.
///
/// These are fields that could expose host filesystem structure, user
/// search intent, or source code content in telemetry (spec §12.2).
pub const SENSITIVE_FIELDS: &[&str] = &[
    // Absolute paths — reveal host filesystem layout.
    "source_root",
    "root",
    // User-supplied query content — could contain code patterns.
    "query_text",
    "pattern",
];

const REDACTED_VALUE: &str = "[REDACTED]";

/// Returns `true` if the given field name is in the redaction policy.
pub fn is_sensitive(field_name: &str) -> bool {
    SENSITIVE_FIELDS.contains(&field_name)
}

// ---------------------------------------------------------------------------
// Field collection with redaction
// ---------------------------------------------------------------------------

/// Collects fields into a `BTreeMap`, redacting sensitive values.
///
/// Using `BTreeMap` instead of `HashMap` ensures deterministic field
/// ordering in JSON output, which simplifies testing and log comparison.
#[derive(Default)]
struct RedactingVisitor {
    fields: BTreeMap<String, serde_json::Value>,
}

impl Visit for RedactingVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let val = if is_sensitive(field.name()) {
            serde_json::Value::String(REDACTED_VALUE.into())
        } else {
            serde_json::Value::String(format!("{value:?}"))
        };
        self.fields.insert(field.name().to_string(), val);
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        let val = if is_sensitive(field.name()) {
            serde_json::Value::String(REDACTED_VALUE.into())
        } else {
            serde_json::Value::String(value.to_string())
        };
        self.fields.insert(field.name().to_string(), val);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        let val = if is_sensitive(field.name()) {
            serde_json::Value::String(REDACTED_VALUE.into())
        } else {
            serde_json::json!(value)
        };
        self.fields.insert(field.name().to_string(), val);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        let val = if is_sensitive(field.name()) {
            serde_json::Value::String(REDACTED_VALUE.into())
        } else {
            serde_json::json!(value)
        };
        self.fields.insert(field.name().to_string(), val);
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        let val = if is_sensitive(field.name()) {
            serde_json::Value::String(REDACTED_VALUE.into())
        } else {
            serde_json::json!(value)
        };
        self.fields.insert(field.name().to_string(), val);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        let val = if is_sensitive(field.name()) {
            serde_json::Value::String(REDACTED_VALUE.into())
        } else {
            serde_json::json!(value)
        };
        self.fields.insert(field.name().to_string(), val);
    }
}

// ---------------------------------------------------------------------------
// Per-span stored fields
// ---------------------------------------------------------------------------

/// Redacted field snapshot stored in a span's extensions.
#[derive(Clone, Default)]
struct SpanFields {
    fields: BTreeMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Redacting field formatter (for compact / fmt layer output)
// ---------------------------------------------------------------------------

/// A [`FormatFields`] implementation that writes fields as `key=value` pairs
/// while replacing sensitive values with `[REDACTED]`.
///
/// Intended for use with `tracing_subscriber::fmt::layer().fmt_fields(...)`,
/// ensuring the compact output mode also enforces the redaction policy.
pub struct RedactingFieldFormatter;

/// Visitor that writes `key=value` pairs directly to a `fmt::Write`,
/// redacting sensitive fields inline.
struct CompactRedactingVisitor<'a> {
    writer: &'a mut dyn fmt::Write,
    first: bool,
    result: fmt::Result,
}

impl<'a> CompactRedactingVisitor<'a> {
    fn new(writer: &'a mut dyn fmt::Write) -> Self {
        Self {
            writer,
            first: true,
            result: Ok(()),
        }
    }

    fn write_separator(&mut self) {
        if !self.first {
            self.result = self.result.and_then(|()| self.writer.write_char(' '));
        }
        self.first = false;
    }
}

impl Visit for CompactRedactingVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.write_separator();
        if is_sensitive(field.name()) {
            self.result = self
                .result
                .and_then(|()| write!(self.writer, "{}={}", field.name(), REDACTED_VALUE));
        } else {
            self.result = self
                .result
                .and_then(|()| write!(self.writer, "{}={:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.write_separator();
        if is_sensitive(field.name()) {
            self.result = self
                .result
                .and_then(|()| write!(self.writer, "{}={}", field.name(), REDACTED_VALUE));
        } else {
            self.result = self
                .result
                .and_then(|()| write!(self.writer, "{}={}", field.name(), value));
        }
    }
}

impl<'writer> FormatFields<'writer> for RedactingFieldFormatter {
    fn format_fields<R: RecordFields>(
        &self,
        mut writer: Writer<'writer>,
        fields: R,
    ) -> fmt::Result {
        let mut visitor = CompactRedactingVisitor::new(&mut writer);
        fields.record(&mut visitor);
        visitor.result
    }
}

// ---------------------------------------------------------------------------
// Redacting JSON layer
// ---------------------------------------------------------------------------

/// A [`tracing_subscriber::Layer`] that emits one JSON line per event,
/// including parent span context, with sensitive field values redacted.
///
/// Output schema (one JSON object per line):
///
/// ```json
/// {
///   "timestamp": "2026-03-10T12:00:00.000Z",
///   "level": "INFO",
///   "target": "indexer::pipeline",
///   "message": "pipeline started",
///   "fields": { "files_discovered": 42 },
///   "spans": [
///     { "name": "index_pipeline", "repo_id": "test", "correlation_id": "[REDACTED]" }
///   ]
/// }
/// ```
pub struct RedactingJsonLayer<W> {
    writer: W,
}

impl<W> RedactingJsonLayer<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W, S> Layer<S> for RedactingJsonLayer<W>
where
    W: for<'a> tracing_subscriber::fmt::MakeWriter<'a> + 'static,
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: Context<'_, S>,
    ) {
        let mut visitor = RedactingVisitor::default();
        attrs.record(&mut visitor);

        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(SpanFields {
                fields: visitor.fields,
            });
        }
    }

    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        ctx: Context<'_, S>,
    ) {
        if let Some(span) = ctx.span(id) {
            let mut visitor = RedactingVisitor::default();
            values.record(&mut visitor);

            let mut ext = span.extensions_mut();
            if let Some(stored) = ext.get_mut::<SpanFields>() {
                stored.fields.extend(visitor.fields);
            } else {
                ext.insert(SpanFields {
                    fields: visitor.fields,
                });
            }
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        // Collect event fields (with redaction).
        let mut visitor = RedactingVisitor::default();
        event.record(&mut visitor);

        // Extract the message from the special `message` field.
        let message = visitor
            .fields
            .remove("message")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();

        // Walk the span stack to build the spans array.
        let mut spans = Vec::new();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope {
                let mut span_obj = BTreeMap::new();
                span_obj.insert(
                    "name".to_string(),
                    serde_json::Value::String(span.name().to_string()),
                );
                if let Some(stored) = span.extensions().get::<SpanFields>() {
                    for (k, v) in &stored.fields {
                        span_obj.insert(k.clone(), v.clone());
                    }
                }
                spans.push(serde_json::Value::Object(span_obj.into_iter().collect()));
            }
        }
        // Reverse so outermost span is first (root → leaf ordering).
        spans.reverse();

        let meta = event.metadata();
        let record = serde_json::json!({
            "timestamp": timestamp_now(),
            "level": meta.level().as_str(),
            "target": meta.target(),
            "message": message,
            "fields": visitor.fields,
            "spans": spans,
        });

        let mut writer = self.writer.make_writer();
        // Ignore write errors — logging must not crash the application.
        let _ = io::Write::write_all(&mut writer, record.to_string().as_bytes());
        let _ = io::Write::write_all(&mut writer, b"\n");
    }
}

/// Returns the current UTC time as an RFC 3339 string.
fn timestamp_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_sensitive_identifies_policy_fields() {
        assert!(is_sensitive("source_root"));
        assert!(is_sensitive("root"));
        assert!(is_sensitive("query_text"));
        assert!(is_sensitive("pattern"));
        assert!(!is_sensitive("repo_id"));
        assert!(!is_sensitive("file_path"));
        assert!(!is_sensitive("level"));
        assert!(!is_sensitive("files_discovered"));
    }

    #[test]
    fn sensitive_fields_list_is_not_empty() {
        assert!(
            !SENSITIVE_FIELDS.is_empty(),
            "redaction policy must define at least one sensitive field"
        );
    }

    #[test]
    fn timestamp_now_returns_rfc3339() {
        let timestamp = timestamp_now();
        let parsed = OffsetDateTime::parse(&timestamp, &Rfc3339);
        assert!(parsed.is_ok(), "timestamp should be RFC3339: {timestamp}");
    }
}
