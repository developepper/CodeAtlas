use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use tracing::{debug, error, info, warn};

use crate::config::TsServerConfig;
use crate::error::TsServerError;
use crate::protocol::{TsServerRequest, TsServerResponse};
use crate::runtime::SemanticRuntime;

/// The operational state of the tsserver process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessState {
    /// The process has not been started or has been stopped.
    Stopped,
    /// The process is running and ready to accept requests.
    Ready,
    /// The process has failed and cannot accept requests.
    Failed { reason: String },
}

impl std::fmt::Display for ProcessState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stopped => write!(f, "stopped"),
            Self::Ready => write!(f, "ready"),
            Self::Failed { reason } => write!(f, "failed: {reason}"),
        }
    }
}

/// Result from the background reader thread: either a successfully read
/// message or an error string that caused the reader to stop.
enum ReaderMsg {
    Message(String),
    Error(String),
}

/// Manages the lifecycle of a tsserver child process.
///
/// Handles spawning, health checking, restart with backoff, timeout
/// enforcement, and clean shutdown. The process communicates via
/// stdin/stdout using the tsserver JSON protocol.
///
/// Protocol I/O is performed on a dedicated reader thread so that
/// timeouts are enforceable even when tsserver stalls mid-message.
pub struct TsServerProcess {
    config: TsServerConfig,
    child: Option<Child>,
    state: ProcessState,
    restart_count: u32,
    sequence: AtomicU32,
    /// Channel receiver for messages read by the background reader thread.
    reader_rx: Option<mpsc::Receiver<ReaderMsg>>,
    /// Handle to the background reader thread, joined on stop.
    reader_handle: Option<thread::JoinHandle<()>>,
    /// Messages peeked from the channel during `is_healthy` that must be
    /// replayed before consuming new channel messages in `recv_response`.
    peek_buffer: Vec<ReaderMsg>,
}

impl TsServerProcess {
    /// Creates a new lifecycle manager with the given configuration.
    ///
    /// The process is not started until [`SemanticRuntime::start`] is called.
    #[must_use]
    pub fn new(config: TsServerConfig) -> Self {
        Self {
            config,
            child: None,
            state: ProcessState::Stopped,
            restart_count: 0,
            sequence: AtomicU32::new(1),
            reader_rx: None,
            reader_handle: None,
            peek_buffer: Vec::new(),
        }
    }

    /// Returns the current state of the tsserver process.
    #[must_use]
    pub fn state(&self) -> &ProcessState {
        &self.state
    }

    /// Returns the number of times the process has been restarted.
    #[must_use]
    pub fn restart_count(&self) -> u32 {
        self.restart_count
    }

    /// Returns the next sequence number for a request.
    fn next_seq(&self) -> u32 {
        self.sequence.fetch_add(1, Ordering::Relaxed)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Spawns the tsserver child process with appropriate isolation settings.
    fn spawn_process(&self) -> Result<Child, TsServerError> {
        let mut cmd = self.build_command();
        cmd.spawn().map_err(|e| TsServerError::SpawnFailed {
            reason: format!(
                "failed to spawn '{}': {e}",
                self.config.tsserver_path.display()
            ),
        })
    }

    /// Builds the `Command` for spawning tsserver, applying resource limits
    /// and isolation settings. Separated from `spawn_process` so that tests
    /// can inspect the command configuration without actually spawning.
    fn build_command(&self) -> Command {
        let mut cmd = Command::new(&self.config.tsserver_path);
        cmd.current_dir(&self.config.working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Apply memory limit via Node.js flag if configured.
        if let Some(mb) = self.config.memory_limit_mb() {
            let current = std::env::var("NODE_OPTIONS").unwrap_or_default();
            let node_opts = format!("{current} --max-old-space-size={mb}")
                .trim()
                .to_string();
            cmd.env("NODE_OPTIONS", node_opts);
        }

        cmd
    }

    /// Starts the background reader thread that owns the `BufReader` over
    /// the child's stdout for the lifetime of the process. Messages are
    /// pushed to an `mpsc` channel so that the main thread can apply
    /// `recv_timeout` for enforceable deadline control.
    fn start_reader_thread(&mut self) -> Result<(), TsServerError> {
        let child = self.child.as_mut().ok_or(TsServerError::InvalidState {
            expected: "running",
            actual: "no child process".to_string(),
        })?;

        let stdout = child.stdout.take().ok_or(TsServerError::Io {
            source: std::io::Error::new(std::io::ErrorKind::BrokenPipe, "stdout not available"),
        })?;

        let (tx, rx) = mpsc::channel();
        self.reader_rx = Some(rx);

        let handle = thread::Builder::new()
            .name("tsserver-reader".to_string())
            .spawn(move || {
                reader_thread_main(stdout, tx);
            })
            .map_err(|e| TsServerError::SpawnFailed {
                reason: format!("failed to spawn reader thread: {e}"),
            })?;

        self.reader_handle = Some(handle);
        Ok(())
    }

    /// Waits for the tsserver process to signal readiness by sending a
    /// `configure` request and confirming the response arrives within
    /// `init_timeout`. This proves the process is alive, the protocol
    /// pipe works end-to-end, and the server can process commands.
    fn wait_for_ready(&mut self) -> Result<(), TsServerError> {
        // Verify the process is still alive after spawn.
        if !self.is_process_alive() {
            return Err(TsServerError::SpawnFailed {
                reason: "process exited immediately after spawn".to_string(),
            });
        }

        // Send a lightweight `configure` request as a health probe.
        let seq = self.next_seq();
        let request = TsServerRequest::with_arguments(seq, "configure", serde_json::json!({}));
        let encoded = request.encode();
        self.write_to_stdin(&encoded)?;

        // Wait for the matching response within init_timeout.
        self.recv_response(seq, self.config.init_timeout)?;
        Ok(())
    }

    /// Checks if the child process is still alive (non-blocking).
    fn is_process_alive(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }

    /// Sends a raw command without tracking response.
    fn send_raw_request(&mut self, command: &str) -> Result<(), TsServerError> {
        let seq = self.next_seq();
        let request = TsServerRequest::new(seq, command);
        let encoded = request.encode();
        self.write_to_stdin(&encoded)
    }

    /// Writes bytes to the child process stdin.
    fn write_to_stdin(&mut self, data: &[u8]) -> Result<(), TsServerError> {
        let child = self.child.as_mut().ok_or(TsServerError::InvalidState {
            expected: "running",
            actual: "no child process".to_string(),
        })?;

        let stdin = child.stdin.as_mut().ok_or(TsServerError::Io {
            source: std::io::Error::new(std::io::ErrorKind::BrokenPipe, "stdin not available"),
        })?;

        stdin.write_all(data)?;
        stdin.flush()?;
        Ok(())
    }

    /// Receives the next response matching `request_seq` from the reader
    /// thread's channel, enforcing `timeout` via `recv_timeout`.
    ///
    /// First replays any messages buffered by `is_healthy`, then reads
    /// from the channel. Skips events and non-matching responses. If the
    /// reader thread reports an error, it is surfaced as a protocol error.
    fn recv_response(
        &mut self,
        request_seq: u32,
        timeout: Duration,
    ) -> Result<TsServerResponse, TsServerError> {
        let deadline = Instant::now() + timeout;

        // Replay any messages that were peeked by is_healthy.
        let buffered = std::mem::take(&mut self.peek_buffer);
        for msg in buffered {
            match msg {
                ReaderMsg::Message(m) => {
                    if let Some(resp) = self.try_match_response(&m, request_seq)? {
                        return Ok(resp);
                    }
                }
                ReaderMsg::Error(reason) => {
                    return Err(TsServerError::Protocol { reason });
                }
            }
        }

        let rx = self.reader_rx.as_ref().ok_or(TsServerError::InvalidState {
            expected: "reader thread running",
            actual: "no reader channel".to_string(),
        })?;

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(TsServerError::Timeout {
                    operation: format!("waiting for response to seq {request_seq}"),
                });
            }

            let msg = match rx.recv_timeout(remaining) {
                Ok(ReaderMsg::Message(m)) => m,
                Ok(ReaderMsg::Error(reason)) => {
                    return Err(TsServerError::Protocol { reason });
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(TsServerError::Timeout {
                        operation: format!("waiting for response to seq {request_seq}"),
                    });
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(TsServerError::Protocol {
                        reason: "reader thread disconnected".to_string(),
                    });
                }
            };

            if let Some(resp) = self.try_match_response(&msg, request_seq)? {
                return Ok(resp);
            }
        }
    }

    /// Tries to parse a raw message as the response matching `request_seq`.
    /// Returns `Ok(Some(response))` on match, `Ok(None)` for non-matching
    /// messages (events, other responses), or `Err` on parse failure.
    fn try_match_response(
        &self,
        raw: &str,
        request_seq: u32,
    ) -> Result<Option<TsServerResponse>, TsServerError> {
        let response: TsServerResponse =
            serde_json::from_str(raw).map_err(|e| TsServerError::Protocol {
                reason: format!("failed to parse response: {e}"),
            })?;

        if response.msg_type == "response" && response.request_seq == Some(request_seq) {
            Ok(Some(response))
        } else {
            Ok(None)
        }
    }

    /// Forcefully kills the child process and joins the reader thread.
    fn force_kill(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.child = None;
        self.reader_rx = None;
        if let Some(handle) = self.reader_handle.take() {
            let _ = handle.join();
        }
    }
}

impl SemanticRuntime for TsServerProcess {
    fn start(&mut self) -> Result<(), TsServerError> {
        if self.state == ProcessState::Ready {
            return Err(TsServerError::InvalidState {
                expected: "stopped or failed",
                actual: self.state.to_string(),
            });
        }

        info!(
            tsserver_path = %self.config.tsserver_path.display(),
            working_dir = %self.config.working_dir.display(),
            "starting tsserver process"
        );

        let child = match self.spawn_process() {
            Ok(c) => c,
            Err(e) => {
                self.state = ProcessState::Failed {
                    reason: e.to_string(),
                };
                return Err(e);
            }
        };
        self.child = Some(child);

        if let Err(e) = self.start_reader_thread() {
            error!(error = %e, "failed to start reader thread");
            self.force_kill();
            self.state = ProcessState::Failed {
                reason: e.to_string(),
            };
            return Err(e);
        }

        match self.wait_for_ready() {
            Ok(()) => {
                self.state = ProcessState::Ready;
                info!("tsserver process is ready");
                Ok(())
            }
            Err(e) => {
                error!(error = %e, "tsserver failed to become ready");
                self.force_kill();
                self.state = ProcessState::Failed {
                    reason: e.to_string(),
                };
                Err(e)
            }
        }
    }

    fn stop(&mut self) {
        if self.child.is_none() {
            self.state = ProcessState::Stopped;
            return;
        }

        info!("stopping tsserver process");

        // Attempt graceful shutdown via exit command.
        let _ = self.send_raw_request("exit");

        // Give the process a moment to exit gracefully.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if Instant::now() >= deadline {
                warn!("tsserver did not exit gracefully, force killing");
                self.force_kill();
                break;
            }
            if let Some(ref mut child) = self.child {
                match child.try_wait() {
                    Ok(Some(_)) => {
                        self.child = None;
                        self.reader_rx = None;
                        if let Some(handle) = self.reader_handle.take() {
                            let _ = handle.join();
                        }
                        break;
                    }
                    Ok(None) => {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => {
                        self.force_kill();
                        break;
                    }
                }
            } else {
                break;
            }
        }

        self.state = ProcessState::Stopped;
        info!("tsserver process stopped");
    }

    fn restart(&mut self) -> Result<(), TsServerError> {
        if self.restart_count >= self.config.max_restarts {
            let err = TsServerError::RestartLimitExceeded {
                attempts: self.restart_count,
            };
            error!(error = %err, "restart limit exceeded");
            self.state = ProcessState::Failed {
                reason: err.to_string(),
            };
            return Err(err);
        }

        self.restart_count += 1;
        warn!(
            restart_count = self.restart_count,
            max_restarts = self.config.max_restarts,
            "restarting tsserver process"
        );

        self.stop();
        self.start()
    }

    fn is_healthy(&mut self) -> bool {
        if self.state != ProcessState::Ready {
            return false;
        }

        let Some(ref mut child) = self.child else {
            self.state = ProcessState::Failed {
                reason: "child process handle missing".to_string(),
            };
            return false;
        };

        // Check if the child process has exited.
        match child.try_wait() {
            Ok(Some(status)) => {
                let reason = format!("process exited with status: {status}");
                warn!(reason = %reason, "tsserver process exited unexpectedly");
                self.state = ProcessState::Failed { reason };
                self.child = None;
                return false;
            }
            Ok(None) => {}
            Err(e) => {
                let reason = format!("failed to check process status: {e}");
                warn!(reason = %reason, "tsserver health check failed");
                self.state = ProcessState::Failed { reason };
                return false;
            }
        }

        // Check if the reader channel is still alive. We do a single
        // non-blocking try_recv to probe the channel state:
        // - Empty     → channel alive, no data waiting, healthy.
        // - Message   → channel alive with data; buffer it so
        //               recv_response can replay it later.
        // - Error     → reader thread reported a fatal read error.
        // - Disconnected → reader thread exited.
        //
        // We must not loop/drain here because that would permanently
        // consume protocol messages that send_request needs.
        if let Some(ref rx) = self.reader_rx {
            match rx.try_recv() {
                // Channel alive with a normal message — preserve it.
                Ok(msg @ ReaderMsg::Message(_)) => {
                    self.peek_buffer.push(msg);
                }
                // Reader thread reported a fatal error — pipe is dead.
                Ok(ReaderMsg::Error(reason)) => {
                    warn!(reason = %reason, "reader channel reported error during health check");
                    self.state = ProcessState::Failed { reason };
                    return false;
                }
                // Channel empty — alive and healthy.
                Err(mpsc::TryRecvError::Empty) => {}
                // Reader thread exited and channel disconnected.
                Err(mpsc::TryRecvError::Disconnected) => {
                    let reason = "reader thread disconnected".to_string();
                    warn!(reason = %reason, "reader channel dead during health check");
                    self.state = ProcessState::Failed { reason };
                    return false;
                }
            }
        } else {
            // No reader channel at all — should not happen in Ready state.
            self.state = ProcessState::Failed {
                reason: "no reader channel".to_string(),
            };
            return false;
        }

        true
    }

    fn send_request(
        &mut self,
        command: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<TsServerResponse, TsServerError> {
        if self.state != ProcessState::Ready {
            return Err(TsServerError::InvalidState {
                expected: "ready",
                actual: self.state.to_string(),
            });
        }

        let seq = self.next_seq();
        let request = match arguments {
            Some(args) => TsServerRequest::with_arguments(seq, command, args),
            None => TsServerRequest::new(seq, command),
        };

        debug!(seq = seq, command = command, "sending tsserver request");

        let encoded = request.encode();
        if let Err(e) = self.write_to_stdin(&encoded) {
            self.state = ProcessState::Failed {
                reason: e.to_string(),
            };
            return Err(e);
        }

        match self.recv_response(seq, self.config.request_timeout) {
            Ok(response) => {
                debug!(seq = seq, success = ?response.success, "received tsserver response");
                Ok(response)
            }
            Err(e @ TsServerError::Protocol { .. }) => {
                warn!(error = %e, "protocol failure, marking runtime as failed");
                self.state = ProcessState::Failed {
                    reason: e.to_string(),
                };
                Err(e)
            }
            Err(e) => Err(e),
        }
    }
}

impl Drop for TsServerProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------
// Background reader thread
// ---------------------------------------------------------------------------

/// Entry point for the background reader thread.
///
/// Owns the `BufReader` over the child's stdout for the entire process
/// lifetime, avoiding the state-corruption bug of per-call BufReader
/// creation. Sends each complete message over the channel so the main
/// thread can apply `recv_timeout` for enforceable deadlines even when
/// the underlying read syscall blocks.
fn reader_thread_main<R: std::io::Read>(stdout: R, tx: mpsc::Sender<ReaderMsg>) {
    let mut reader = BufReader::new(stdout);
    let mut header_line = String::new();

    loop {
        header_line.clear();
        let bytes_read = match reader.read_line(&mut header_line) {
            Ok(n) => n,
            Err(e) => {
                let _ = tx.send(ReaderMsg::Error(format!("read error: {e}")));
                return;
            }
        };

        if bytes_read == 0 {
            // EOF: child closed stdout.
            let _ = tx.send(ReaderMsg::Error("unexpected end of stream".to_string()));
            return;
        }

        let trimmed = header_line.trim();

        // Skip blank lines between messages.
        if trimmed.is_empty() {
            continue;
        }

        // Content-Length framed message.
        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            let content_length: usize = match len_str.parse() {
                Ok(n) => n,
                Err(e) => {
                    let _ = tx.send(ReaderMsg::Error(format!("invalid Content-Length: {e}")));
                    return;
                }
            };

            // Read the blank line separator.
            let mut separator = String::new();
            if let Err(e) = reader.read_line(&mut separator) {
                let _ = tx.send(ReaderMsg::Error(format!("read error: {e}")));
                return;
            }

            // Read exactly content_length bytes of body.
            let mut body = vec![0u8; content_length];
            if let Err(e) = std::io::Read::read_exact(&mut reader, &mut body) {
                let _ = tx.send(ReaderMsg::Error(format!("read error: {e}")));
                return;
            }

            let msg = match String::from_utf8(body) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(ReaderMsg::Error(format!(
                        "response body is not valid UTF-8: {e}"
                    )));
                    return;
                }
            };

            if tx.send(ReaderMsg::Message(msg)).is_err() {
                return; // Receiver dropped, process shutting down.
            }
            continue;
        }

        // tsserver may emit plain JSON lines without Content-Length framing.
        if trimmed.starts_with('{') {
            if tx.send(ReaderMsg::Message(trimmed.to_string())).is_err() {
                return;
            }
            continue;
        }

        // Unknown line format — skip silently (tsserver may emit logging).
    }
}

// ---------------------------------------------------------------------------
// Standalone message parser for unit testing
// ---------------------------------------------------------------------------

/// Reads a single tsserver message from a reader. Used only in unit tests
/// to validate framing logic without the channel infrastructure.
#[cfg(test)]
fn read_message<R: BufRead>(reader: &mut R, deadline: Instant) -> Result<String, TsServerError> {
    let mut header_line = String::new();
    loop {
        if Instant::now() >= deadline {
            return Err(TsServerError::Timeout {
                operation: "reading message header".to_string(),
            });
        }

        header_line.clear();
        let bytes_read = reader
            .read_line(&mut header_line)
            .map_err(TsServerError::from)?;
        if bytes_read == 0 {
            return Err(TsServerError::Protocol {
                reason: "unexpected end of stream".to_string(),
            });
        }

        let trimmed = header_line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            let content_length: usize = len_str.parse().map_err(|e| TsServerError::Protocol {
                reason: format!("invalid Content-Length: {e}"),
            })?;

            let mut separator = String::new();
            reader
                .read_line(&mut separator)
                .map_err(TsServerError::from)?;

            let mut body = vec![0u8; content_length];
            std::io::Read::read_exact(reader, &mut body).map_err(TsServerError::from)?;

            return String::from_utf8(body).map_err(|e| TsServerError::Protocol {
                reason: format!("response body is not valid UTF-8: {e}"),
            });
        }

        if trimmed.starts_with('{') {
            return Ok(trimmed.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::SemanticRuntime;
    use std::path::PathBuf;

    fn test_config() -> TsServerConfig {
        TsServerConfig::new(
            PathBuf::from("/usr/bin/tsserver"),
            PathBuf::from("/tmp/test-repo"),
        )
    }

    #[test]
    fn new_process_starts_in_stopped_state() {
        let proc = TsServerProcess::new(test_config());
        assert_eq!(*proc.state(), ProcessState::Stopped);
        assert_eq!(proc.restart_count(), 0);
    }

    #[test]
    fn start_with_invalid_binary_returns_spawn_failed() {
        let config = TsServerConfig::new(
            PathBuf::from("/nonexistent/tsserver-binary-that-does-not-exist"),
            PathBuf::from("/tmp"),
        );
        let mut proc = TsServerProcess::new(config);
        let err = proc.start().expect_err("should fail to spawn");
        assert!(
            matches!(err, TsServerError::SpawnFailed { .. }),
            "expected SpawnFailed, got: {err}"
        );
        assert!(
            matches!(proc.state(), ProcessState::Failed { .. }),
            "expected failed state"
        );
    }

    #[test]
    fn start_when_already_ready_returns_invalid_state() {
        let config = test_config();
        let mut proc = TsServerProcess::new(config);
        proc.state = ProcessState::Ready;

        let err = proc.start().expect_err("should reject double-start");
        assert!(
            matches!(err, TsServerError::InvalidState { .. }),
            "expected InvalidState, got: {err}"
        );
    }

    #[test]
    fn stop_from_stopped_is_idempotent() {
        let mut proc = TsServerProcess::new(test_config());
        proc.stop();
        assert_eq!(*proc.state(), ProcessState::Stopped);
    }

    #[test]
    fn restart_limit_exceeded_returns_error() {
        let config = TsServerConfig::new(
            PathBuf::from("/nonexistent/tsserver"),
            PathBuf::from("/tmp"),
        )
        .with_max_restarts(2);

        let mut proc = TsServerProcess::new(config);
        proc.restart_count = 2;

        let err = proc.restart().expect_err("should exceed restart limit");
        assert!(
            matches!(err, TsServerError::RestartLimitExceeded { attempts: 2 }),
            "expected RestartLimitExceeded, got: {err}"
        );
        assert!(
            matches!(proc.state(), ProcessState::Failed { .. }),
            "expected failed state after restart limit exceeded"
        );
    }

    #[test]
    fn is_healthy_returns_false_when_stopped() {
        let mut proc = TsServerProcess::new(test_config());
        assert!(!proc.is_healthy());
    }

    #[test]
    fn is_healthy_returns_false_when_failed() {
        let mut proc = TsServerProcess::new(test_config());
        proc.state = ProcessState::Failed {
            reason: "test failure".to_string(),
        };
        assert!(!proc.is_healthy());
    }

    #[test]
    fn is_healthy_detects_missing_child() {
        let mut proc = TsServerProcess::new(test_config());
        proc.state = ProcessState::Ready;
        proc.child = None;

        assert!(!proc.is_healthy());
        assert!(
            matches!(proc.state(), ProcessState::Failed { .. }),
            "state should transition to failed"
        );
    }

    #[test]
    fn send_request_fails_when_not_ready() {
        let mut proc = TsServerProcess::new(test_config());
        let err = proc
            .send_request("open", None)
            .expect_err("should reject request when stopped");
        assert!(
            matches!(err, TsServerError::InvalidState { .. }),
            "expected InvalidState, got: {err}"
        );
    }

    #[test]
    fn process_state_display() {
        assert_eq!(ProcessState::Stopped.to_string(), "stopped");
        assert_eq!(ProcessState::Ready.to_string(), "ready");
        assert_eq!(
            ProcessState::Failed {
                reason: "crash".to_string()
            }
            .to_string(),
            "failed: crash"
        );
    }

    #[test]
    fn read_message_parses_content_length_framing() {
        let data = "Content-Length: 27\r\n\r\n{\"seq\":0,\"type\":\"response\"}";
        let mut reader = std::io::BufReader::new(data.as_bytes());
        let deadline = Instant::now() + Duration::from_secs(5);

        let msg = read_message(&mut reader, deadline).expect("should parse message");
        assert_eq!(msg, r#"{"seq":0,"type":"response"}"#);
    }

    #[test]
    fn read_message_parses_plain_json_line() {
        let data = "{\"seq\":0,\"type\":\"event\",\"event\":\"test\"}\n";
        let mut reader = std::io::BufReader::new(data.as_bytes());
        let deadline = Instant::now() + Duration::from_secs(5);

        let msg = read_message(&mut reader, deadline).expect("should parse plain JSON");
        assert!(msg.contains("\"event\":\"test\""));
    }

    #[test]
    fn read_message_handles_eof() {
        let data = "";
        let mut reader = std::io::BufReader::new(data.as_bytes());
        let deadline = Instant::now() + Duration::from_secs(5);

        let err = read_message(&mut reader, deadline).expect_err("should fail on EOF");
        assert!(matches!(err, TsServerError::Protocol { .. }));
    }

    #[test]
    fn spawn_with_short_lived_process_detects_exit() {
        // Use `true` which exits immediately — simulates tsserver crash on startup.
        let config = TsServerConfig::new(PathBuf::from("true"), PathBuf::from("/tmp"))
            .with_init_timeout(Duration::from_secs(2));

        let mut proc = TsServerProcess::new(config);
        let result = proc.start();

        // The process will exit immediately; start should detect this
        // either via wait_for_ready failure or post-start health check.
        if result.is_ok() {
            std::thread::sleep(Duration::from_millis(100));
            assert!(!proc.is_healthy());
        }
    }

    #[test]
    fn sequence_numbers_increment() {
        let proc = TsServerProcess::new(test_config());
        let s1 = proc.next_seq();
        let s2 = proc.next_seq();
        let s3 = proc.next_seq();
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
        assert_eq!(s3, 3);
    }

    #[test]
    fn max_restarts_zero_disables_restart() {
        let config = TsServerConfig::new(
            PathBuf::from("/nonexistent/tsserver"),
            PathBuf::from("/tmp"),
        )
        .with_max_restarts(0);

        let mut proc = TsServerProcess::new(config);
        let err = proc.restart().expect_err("should fail with max_restarts=0");
        assert!(matches!(err, TsServerError::RestartLimitExceeded { .. }));
    }

    // -- Reader thread channel tests --

    #[test]
    fn reader_thread_sends_content_length_message() {
        let data = b"Content-Length: 27\r\n\r\n{\"seq\":0,\"type\":\"response\"}";
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            reader_thread_main(&data[..], tx);
        });

        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(ReaderMsg::Message(msg)) => {
                assert_eq!(msg, r#"{"seq":0,"type":"response"}"#);
            }
            Ok(ReaderMsg::Error(e)) => panic!("unexpected error: {e}"),
            Err(e) => panic!("recv failed: {e}"),
        }
        handle.join().unwrap();
    }

    #[test]
    fn reader_thread_sends_plain_json_line() {
        let data = b"{\"seq\":0,\"type\":\"event\",\"event\":\"test\"}\n";
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            reader_thread_main(&data[..], tx);
        });

        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(ReaderMsg::Message(msg)) => {
                assert!(msg.contains("\"event\":\"test\""));
            }
            Ok(ReaderMsg::Error(e)) => panic!("unexpected error: {e}"),
            Err(e) => panic!("recv failed: {e}"),
        }
        handle.join().unwrap();
    }

    #[test]
    fn reader_thread_reports_eof_as_error() {
        let data = b"";
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            reader_thread_main(&data[..], tx);
        });

        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(ReaderMsg::Error(reason)) => {
                assert!(reason.contains("end of stream"), "got: {reason}");
            }
            Ok(ReaderMsg::Message(m)) => panic!("unexpected message: {m}"),
            Err(e) => panic!("recv failed: {e}"),
        }
        handle.join().unwrap();
    }

    #[test]
    fn reader_thread_handles_multiple_messages() {
        // Two Content-Length framed messages back-to-back.
        let msg1 = r#"{"seq":0,"type":"event","event":"a"}"#;
        let msg2 = r#"{"seq":1,"type":"response","request_seq":1,"success":true}"#;
        let data = format!(
            "Content-Length: {}\r\n\r\n{}Content-Length: {}\r\n\r\n{}",
            msg1.len(),
            msg1,
            msg2.len(),
            msg2
        );

        let (tx, rx) = mpsc::channel();
        let data_bytes = data.into_bytes();
        let handle = thread::spawn(move || {
            reader_thread_main(&data_bytes[..], tx);
        });

        let m1 = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let m2 = rx.recv_timeout(Duration::from_secs(2)).unwrap();

        match m1 {
            ReaderMsg::Message(s) => assert!(s.contains("\"event\":\"a\""), "got: {s}"),
            ReaderMsg::Error(e) => panic!("unexpected error: {e}"),
        }
        match m2 {
            ReaderMsg::Message(s) => assert!(s.contains("\"success\":true"), "got: {s}"),
            ReaderMsg::Error(e) => panic!("unexpected error: {e}"),
        }

        handle.join().unwrap();
    }

    #[test]
    fn recv_response_times_out_on_empty_channel() {
        let (tx, rx) = mpsc::channel::<ReaderMsg>();
        let mut proc = TsServerProcess {
            config: test_config(),
            child: None,
            state: ProcessState::Ready,
            restart_count: 0,
            sequence: AtomicU32::new(1),
            reader_rx: Some(rx),
            reader_handle: None,
            peek_buffer: Vec::new(),
        };

        let err = proc
            .recv_response(1, Duration::from_millis(50))
            .expect_err("should time out");
        assert!(
            matches!(err, TsServerError::Timeout { .. }),
            "expected Timeout, got: {err}"
        );
        drop(tx); // Keep sender alive until after the assertion.
    }

    #[test]
    fn recv_response_returns_disconnected_when_reader_gone() {
        let (tx, rx) = mpsc::channel::<ReaderMsg>();
        drop(tx); // Immediately disconnect.

        let mut proc = TsServerProcess {
            config: test_config(),
            child: None,
            state: ProcessState::Ready,
            restart_count: 0,
            sequence: AtomicU32::new(1),
            reader_rx: Some(rx),
            reader_handle: None,
            peek_buffer: Vec::new(),
        };

        let err = proc
            .recv_response(1, Duration::from_secs(1))
            .expect_err("should detect disconnect");
        assert!(
            matches!(err, TsServerError::Protocol { .. }),
            "expected Protocol error for disconnect, got: {err}"
        );
    }

    #[test]
    fn recv_response_skips_events_and_non_matching() {
        let (tx, rx) = mpsc::channel();

        // Send an event, a non-matching response, then the matching response.
        tx.send(ReaderMsg::Message(
            r#"{"seq":0,"type":"event","event":"telemetry"}"#.to_string(),
        ))
        .unwrap();
        tx.send(ReaderMsg::Message(
            r#"{"seq":1,"type":"response","request_seq":99,"success":true}"#.to_string(),
        ))
        .unwrap();
        tx.send(ReaderMsg::Message(
            r#"{"seq":2,"type":"response","request_seq":5,"success":true}"#.to_string(),
        ))
        .unwrap();

        let mut proc = TsServerProcess {
            config: test_config(),
            child: None,
            state: ProcessState::Ready,
            restart_count: 0,
            sequence: AtomicU32::new(1),
            reader_rx: Some(rx),
            reader_handle: None,
            peek_buffer: Vec::new(),
        };

        let resp = proc
            .recv_response(5, Duration::from_secs(2))
            .expect("should find matching response");
        assert!(resp.is_success_for(5));
    }

    #[test]
    fn recv_response_surfaces_reader_error() {
        let (tx, rx) = mpsc::channel();
        tx.send(ReaderMsg::Error("simulated read failure".to_string()))
            .unwrap();

        let mut proc = TsServerProcess {
            config: test_config(),
            child: None,
            state: ProcessState::Ready,
            restart_count: 0,
            sequence: AtomicU32::new(1),
            reader_rx: Some(rx),
            reader_handle: None,
            peek_buffer: Vec::new(),
        };

        let err = proc
            .recv_response(1, Duration::from_secs(2))
            .expect_err("should surface reader error");
        match err {
            TsServerError::Protocol { reason } => {
                assert!(reason.contains("simulated read failure"));
            }
            other => panic!("expected Protocol error, got: {other}"),
        }
    }

    // -- Finding 1: send_request transitions to Failed on protocol error --

    #[test]
    fn send_request_transitions_to_failed_on_reader_disconnect() {
        let (tx, rx) = mpsc::channel::<ReaderMsg>();
        drop(tx); // Reader gone immediately.

        let mut proc = TsServerProcess {
            config: test_config(),
            child: None,
            state: ProcessState::Ready,
            restart_count: 0,
            sequence: AtomicU32::new(1),
            reader_rx: Some(rx),
            reader_handle: None,
            peek_buffer: Vec::new(),
        };

        // Manually set up a fake stdin so write_to_stdin doesn't fail
        // before we reach recv_response. Use a real process for the pipe.
        let child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        proc.child = Some(child);

        let err = proc
            .send_request("open", None)
            .expect_err("should fail on disconnect");
        assert!(
            matches!(err, TsServerError::Protocol { .. }),
            "expected Protocol, got: {err}"
        );
        assert!(
            matches!(proc.state(), ProcessState::Failed { .. }),
            "state should transition to Failed after protocol error"
        );
    }

    #[test]
    fn send_request_transitions_to_failed_on_reader_error() {
        let (tx, rx) = mpsc::channel();
        tx.send(ReaderMsg::Error("pipe broken".to_string()))
            .unwrap();

        let mut proc = TsServerProcess {
            config: test_config(),
            child: None,
            state: ProcessState::Ready,
            restart_count: 0,
            sequence: AtomicU32::new(1),
            reader_rx: Some(rx),
            reader_handle: None,
            peek_buffer: Vec::new(),
        };

        let child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        proc.child = Some(child);

        let err = proc
            .send_request("open", None)
            .expect_err("should fail on reader error");
        assert!(
            matches!(err, TsServerError::Protocol { .. }),
            "expected Protocol, got: {err}"
        );
        assert!(
            matches!(proc.state(), ProcessState::Failed { .. }),
            "state should transition to Failed"
        );
    }

    // -- Finding 2: is_healthy detects reader channel failures --

    #[test]
    fn is_healthy_detects_reader_disconnect() {
        let (tx, rx) = mpsc::channel::<ReaderMsg>();
        drop(tx); // Reader thread gone.

        let child = Command::new("sleep")
            .arg("60")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let mut proc = TsServerProcess {
            config: test_config(),
            child: Some(child),
            state: ProcessState::Ready,
            restart_count: 0,
            sequence: AtomicU32::new(1),
            reader_rx: Some(rx),
            reader_handle: None,
            peek_buffer: Vec::new(),
        };

        assert!(
            !proc.is_healthy(),
            "should detect disconnected reader channel"
        );
        assert!(
            matches!(proc.state(), ProcessState::Failed { .. }),
            "state should transition to Failed"
        );
    }

    #[test]
    fn is_healthy_detects_reader_error_message() {
        let (tx, rx) = mpsc::channel();
        tx.send(ReaderMsg::Error("stdout read error".to_string()))
            .unwrap();

        let child = Command::new("sleep")
            .arg("60")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let mut proc = TsServerProcess {
            config: test_config(),
            child: Some(child),
            state: ProcessState::Ready,
            restart_count: 0,
            sequence: AtomicU32::new(1),
            reader_rx: Some(rx),
            reader_handle: None,
            peek_buffer: Vec::new(),
        };

        assert!(!proc.is_healthy(), "should detect reader error");
        assert!(
            matches!(proc.state(), ProcessState::Failed { .. }),
            "state should transition to Failed"
        );
    }

    #[test]
    fn is_healthy_returns_false_with_no_reader_channel() {
        let child = Command::new("sleep")
            .arg("60")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let mut proc = TsServerProcess {
            config: test_config(),
            child: Some(child),
            state: ProcessState::Ready,
            restart_count: 0,
            sequence: AtomicU32::new(1),
            reader_rx: None,
            reader_handle: None,
            peek_buffer: Vec::new(),
        };

        assert!(
            !proc.is_healthy(),
            "should fail when reader_rx is None in Ready state"
        );
        assert!(
            matches!(proc.state(), ProcessState::Failed { .. }),
            "state should transition to Failed"
        );
    }

    // -- Finding 3: build_command applies resource limits to environment --

    #[test]
    fn build_command_sets_node_options_when_memory_limit_configured() {
        let config = TsServerConfig::new(
            PathBuf::from("/usr/local/bin/tsserver"),
            PathBuf::from("/tmp/repo"),
        )
        .with_memory_limit(512 * 1024 * 1024);

        let proc = TsServerProcess::new(config);
        let cmd = proc.build_command();

        // Command::get_envs() returns the overridden env vars.
        let node_opts: Vec<_> = cmd
            .get_envs()
            .filter(|(k, _)| k == &std::ffi::OsStr::new("NODE_OPTIONS"))
            .collect();
        assert_eq!(node_opts.len(), 1, "NODE_OPTIONS should be set");

        let val = node_opts[0]
            .1
            .expect("NODE_OPTIONS should have a value")
            .to_str()
            .unwrap();
        assert!(
            val.contains("--max-old-space-size=512"),
            "expected --max-old-space-size=512 in NODE_OPTIONS, got: {val}"
        );
    }

    #[test]
    fn build_command_does_not_set_node_options_without_memory_limit() {
        let config = TsServerConfig::new(
            PathBuf::from("/usr/local/bin/tsserver"),
            PathBuf::from("/tmp/repo"),
        );

        let proc = TsServerProcess::new(config);
        let cmd = proc.build_command();

        let node_opts: Vec<_> = cmd
            .get_envs()
            .filter(|(k, _)| k == &std::ffi::OsStr::new("NODE_OPTIONS"))
            .collect();
        assert!(
            node_opts.is_empty(),
            "NODE_OPTIONS should not be set without memory limit"
        );
    }

    #[test]
    fn build_command_sets_working_directory() {
        let config = TsServerConfig::new(
            PathBuf::from("/usr/local/bin/tsserver"),
            PathBuf::from("/my/project/root"),
        );

        let proc = TsServerProcess::new(config);
        let cmd = proc.build_command();

        assert_eq!(
            cmd.get_current_dir(),
            Some(std::path::Path::new("/my/project/root")),
            "working directory should match config"
        );
    }

    #[test]
    fn build_command_uses_configured_binary_path() {
        let config = TsServerConfig::new(
            PathBuf::from("/custom/path/to/tsserver"),
            PathBuf::from("/tmp"),
        );

        let proc = TsServerProcess::new(config);
        let cmd = proc.build_command();

        assert_eq!(
            cmd.get_program(),
            std::ffi::OsStr::new("/custom/path/to/tsserver"),
            "program should match configured tsserver_path"
        );
    }

    // -- Message preservation across health checks --

    #[test]
    fn is_healthy_does_not_consume_queued_messages() {
        let (tx, rx) = mpsc::channel();

        // Queue a response message before the health check.
        let response_json =
            r#"{"seq":1,"type":"response","request_seq":7,"success":true}"#.to_string();
        tx.send(ReaderMsg::Message(response_json)).unwrap();

        let child = Command::new("sleep")
            .arg("60")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let mut proc = TsServerProcess {
            config: test_config(),
            child: Some(child),
            state: ProcessState::Ready,
            restart_count: 0,
            sequence: AtomicU32::new(1),
            reader_rx: Some(rx),
            reader_handle: None,
            peek_buffer: Vec::new(),
        };

        // Health check should succeed and NOT consume the message.
        assert!(proc.is_healthy(), "process should be healthy");

        // The message should be in the peek buffer, ready for recv_response.
        assert_eq!(
            proc.peek_buffer.len(),
            1,
            "peeked message should be buffered"
        );

        // recv_response should find the buffered message.
        let resp = proc
            .recv_response(7, Duration::from_secs(2))
            .expect("should find the buffered response");
        assert!(resp.is_success_for(7));

        // Buffer should be drained after consumption.
        assert!(
            proc.peek_buffer.is_empty(),
            "peek buffer should be empty after recv_response"
        );
    }

    #[test]
    fn is_healthy_preserves_message_then_recv_response_reads_from_channel() {
        let (tx, rx) = mpsc::channel();

        // Queue: event (will be peeked by health check), then matching response.
        let event_json = r#"{"seq":0,"type":"event","event":"telemetry"}"#.to_string();
        let response_json =
            r#"{"seq":1,"type":"response","request_seq":3,"success":true}"#.to_string();
        tx.send(ReaderMsg::Message(event_json)).unwrap();
        tx.send(ReaderMsg::Message(response_json)).unwrap();

        let child = Command::new("sleep")
            .arg("60")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let mut proc = TsServerProcess {
            config: test_config(),
            child: Some(child),
            state: ProcessState::Ready,
            restart_count: 0,
            sequence: AtomicU32::new(1),
            reader_rx: Some(rx),
            reader_handle: None,
            peek_buffer: Vec::new(),
        };

        // Health check peeks the event.
        assert!(proc.is_healthy());
        assert_eq!(proc.peek_buffer.len(), 1);

        // recv_response replays the event (non-matching), then reads the
        // response from the channel.
        let resp = proc
            .recv_response(3, Duration::from_secs(2))
            .expect("should find response after replaying buffered event");
        assert!(resp.is_success_for(3));
    }

    #[test]
    fn multiple_health_checks_accumulate_buffered_messages() {
        let (tx, rx) = mpsc::channel();

        tx.send(ReaderMsg::Message(
            r#"{"seq":0,"type":"event","event":"a"}"#.to_string(),
        ))
        .unwrap();
        tx.send(ReaderMsg::Message(
            r#"{"seq":1,"type":"event","event":"b"}"#.to_string(),
        ))
        .unwrap();
        tx.send(ReaderMsg::Message(
            r#"{"seq":2,"type":"response","request_seq":10,"success":true}"#.to_string(),
        ))
        .unwrap();

        let child = Command::new("sleep")
            .arg("60")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let mut proc = TsServerProcess {
            config: test_config(),
            child: Some(child),
            state: ProcessState::Ready,
            restart_count: 0,
            sequence: AtomicU32::new(1),
            reader_rx: Some(rx),
            reader_handle: None,
            peek_buffer: Vec::new(),
        };

        // Two health checks peek two messages.
        assert!(proc.is_healthy());
        assert_eq!(proc.peek_buffer.len(), 1);
        assert!(proc.is_healthy());
        assert_eq!(proc.peek_buffer.len(), 2);

        // recv_response replays both buffered events, then reads the
        // matching response from the channel.
        let resp = proc
            .recv_response(10, Duration::from_secs(2))
            .expect("should find response after replaying multiple buffered events");
        assert!(resp.is_success_for(10));
        assert!(proc.peek_buffer.is_empty());
    }
}
