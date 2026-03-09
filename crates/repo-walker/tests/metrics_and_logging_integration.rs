use std::sync::{Arc, Mutex};

use repo_walker::{walk_repository, WalkerOptions};
use tracing_subscriber::fmt::MakeWriter;

mod common;
use common::FixtureRepo;

/// A writer that captures all output into a shared buffer.
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
        self.buffer
            .lock()
            .expect("lock buffer")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for CapturedWriter {
    type Writer = CapturedWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[test]
fn structured_log_contains_expected_fields() {
    let fixture = FixtureRepo::new().expect("create fixture repo");
    fixture
        .write("src/main.rs", "fn main() {}\n")
        .expect("write text");
    fixture
        .write_bytes("bin/data.bin", &[0, 1, 2])
        .expect("write binary");

    let writer = CapturedWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_writer(writer.clone())
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        let _result =
            walk_repository(fixture.path(), &WalkerOptions::default()).expect("walk repo");
    });

    let output = writer.contents();
    // The "discovery walk completed" log line must contain all metric fields.
    let completed_line = output
        .lines()
        .find(|line| line.contains("discovery walk completed"))
        .expect("should have a 'discovery walk completed' log line");

    // Verify structured field presence in the JSON log line.
    for field in &[
        "files_discovered",
        "files_skipped_git_dir",
        "files_skipped_symlink",
        "files_skipped_extra_rules",
        "files_skipped_size",
        "files_skipped_binary",
        "total_entries_evaluated",
        "walk_duration_ms",
    ] {
        assert!(
            completed_line.contains(field),
            "expected field '{field}' in log line: {completed_line}"
        );
    }

    // Verify the "discovery walk started" log is also emitted.
    assert!(
        output
            .lines()
            .any(|line| line.contains("discovery walk started")),
        "should have a 'discovery walk started' log line"
    );

    // All log lines must carry the correlation_id span field (spec 13.2).
    for line in output.lines() {
        assert!(
            line.contains("correlation_id"),
            "log line missing 'correlation_id' span field: {line}"
        );
    }
}

#[test]
fn correlation_id_propagates_to_all_log_events() {
    let fixture = FixtureRepo::new().expect("create fixture repo");
    fixture
        .write("src/main.rs", "fn main() {}\n")
        .expect("write text");
    fixture
        .write_bytes("bin/data.bin", &[0, 1, 2])
        .expect("write binary");

    let writer = CapturedWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_writer(writer.clone())
        .with_max_level(tracing::Level::DEBUG)
        .finish();

    let options = WalkerOptions {
        correlation_id: Some("job-42".to_string()),
        ..WalkerOptions::default()
    };

    tracing::subscriber::with_default(subscriber, || {
        let _result = walk_repository(fixture.path(), &options).expect("walk repo");
    });

    let output = writer.contents();
    assert!(!output.is_empty(), "should have log output");

    // Every log line must carry the caller-supplied correlation ID value.
    for line in output.lines() {
        assert!(
            line.contains("job-42"),
            "log line missing correlation_id value 'job-42': {line}"
        );
    }
}

#[test]
fn debug_logs_emit_skip_reason_per_file() {
    let fixture = FixtureRepo::new().expect("create fixture repo");
    fixture
        .write("src/main.rs", "fn main() {}\n")
        .expect("write text");
    fixture
        .write_bytes("bin/data.bin", &[0, 1, 2])
        .expect("write binary");
    fixture
        .write("big.txt", &"x".repeat(200))
        .expect("write big");

    let writer = CapturedWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_writer(writer.clone())
        .with_max_level(tracing::Level::DEBUG)
        .finish();

    let options = WalkerOptions {
        max_file_size_bytes: Some(100),
        ..WalkerOptions::default()
    };

    tracing::subscriber::with_default(subscriber, || {
        let _result = walk_repository(fixture.path(), &options).expect("walk repo");
    });

    let output = writer.contents();
    let skip_lines: Vec<&str> = output
        .lines()
        .filter(|line| line.contains("file skipped"))
        .collect();

    // Should have skip events for binary and size
    assert!(
        skip_lines
            .iter()
            .any(|line| line.contains("\"reason\":\"binary\"")),
        "expected a binary skip event in: {skip_lines:?}"
    );
    assert!(
        skip_lines
            .iter()
            .any(|line| line.contains("\"reason\":\"size_cap\"")),
        "expected a size_cap skip event in: {skip_lines:?}"
    );

    // Each skip event must include a path field
    for line in &skip_lines {
        assert!(
            line.contains("path"),
            "skip event missing 'path' field: {line}"
        );
    }
}

#[test]
fn metrics_deterministic_across_repeated_walks() {
    let fixture = FixtureRepo::new().expect("create fixture repo");
    fixture.write("a.rs", "fn a() {}\n").expect("write file");
    fixture.write("b.rs", "fn b() {}\n").expect("write file");
    fixture
        .write_bytes("c.bin", &[0, 1, 2])
        .expect("write binary");

    let opts = WalkerOptions::default();
    let r1 = walk_repository(fixture.path(), &opts).expect("walk 1");
    let r2 = walk_repository(fixture.path(), &opts).expect("walk 2");

    // Deterministic counters must match exactly.
    assert_eq!(r1.metrics.files_discovered, r2.metrics.files_discovered);
    assert_eq!(
        r1.metrics.files_skipped_binary,
        r2.metrics.files_skipped_binary
    );
    assert_eq!(
        r1.metrics.total_entries_evaluated(),
        r2.metrics.total_entries_evaluated()
    );
}
