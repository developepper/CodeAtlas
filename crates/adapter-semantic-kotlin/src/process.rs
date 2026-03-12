use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use tracing::{debug, error, info, warn};

use crate::config::KotlinAnalysisConfig;
use crate::error::KotlinAnalysisError;
use crate::protocol::{KotlinRequest, KotlinResponse};
use crate::runtime::KotlinRuntime;

/// The operational state of the Kotlin analysis bridge process.
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

/// Result from the background reader thread.
enum ReaderMsg {
    Message(String),
    Error(String),
}

/// Manages the lifecycle of a Kotlin analysis bridge JVM subprocess.
///
/// Handles spawning, health checking, restart with backoff, timeout
/// enforcement, and clean shutdown. The process communicates via
/// stdin/stdout using Content-Length framed JSON protocol.
///
/// Protocol I/O is performed on a dedicated reader thread so that
/// timeouts are enforceable even when the JVM stalls mid-message.
pub struct KotlinAnalysisProcess {
    config: KotlinAnalysisConfig,
    child: Option<Child>,
    pub(crate) state: ProcessState,
    pub(crate) restart_count: u32,
    sequence: AtomicU32,
    reader_rx: Option<mpsc::Receiver<ReaderMsg>>,
    reader_handle: Option<thread::JoinHandle<()>>,
    peek_buffer: Vec<ReaderMsg>,
}

impl KotlinAnalysisProcess {
    /// Creates a new lifecycle manager with the given configuration.
    ///
    /// The process is not started until [`KotlinRuntime::start`] is called.
    #[must_use]
    pub fn new(config: KotlinAnalysisConfig) -> Self {
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

    /// Returns the current state of the process.
    #[must_use]
    pub fn state(&self) -> &ProcessState {
        &self.state
    }

    /// Returns the number of times the process has been restarted.
    #[must_use]
    pub fn restart_count(&self) -> u32 {
        self.restart_count
    }

    fn next_seq(&self) -> u32 {
        self.sequence.fetch_add(1, Ordering::Relaxed)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn spawn_process(&self) -> Result<Child, KotlinAnalysisError> {
        // Retry on ETXTBSY (errno 26 on Linux) — a transient race that
        // occurs on overlayfs/tmpfs when the kernel hasn't fully released
        // the write reference to an executable we just finished writing.
        let mut last_err = None;
        for attempt in 0..5 {
            let mut cmd = self.build_command();
            match cmd.spawn() {
                Ok(child) => return Ok(child),
                Err(e) if e.raw_os_error() == Some(26) && attempt < 4 => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    last_err = Some(e);
                }
                Err(e) => {
                    last_err = Some(e);
                    break;
                }
            }
        }
        Err(KotlinAnalysisError::SpawnFailed {
            reason: format!(
                "failed to spawn java -jar '{}': {}",
                self.config.bridge_jar_path.display(),
                last_err.unwrap()
            ),
        })
    }

    /// Builds the `Command` for spawning the JVM bridge process.
    fn build_command(&self) -> Command {
        let mut cmd = Command::new(&self.config.java_path);

        // Apply heap limit via -Xmx if configured.
        if let Some(mb) = self.config.heap_limit_mb() {
            cmd.arg(format!("-Xmx{mb}m"));
        }

        cmd.arg("-jar")
            .arg(&self.config.bridge_jar_path)
            .current_dir(&self.config.working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        cmd
    }

    fn start_reader_thread(&mut self) -> Result<(), KotlinAnalysisError> {
        let child = self
            .child
            .as_mut()
            .ok_or(KotlinAnalysisError::InvalidState {
                expected: "running",
                actual: "no child process".to_string(),
            })?;

        let stdout = child.stdout.take().ok_or(KotlinAnalysisError::Io {
            source: std::io::Error::new(std::io::ErrorKind::BrokenPipe, "stdout not available"),
        })?;

        let (tx, rx) = mpsc::channel();
        self.reader_rx = Some(rx);

        let handle = thread::Builder::new()
            .name("kotlin-bridge-reader".to_string())
            .spawn(move || {
                reader_thread_main(stdout, tx);
            })
            .map_err(|e| KotlinAnalysisError::SpawnFailed {
                reason: format!("failed to spawn reader thread: {e}"),
            })?;

        self.reader_handle = Some(handle);
        Ok(())
    }

    /// Waits for the bridge to signal readiness by sending a `ping`
    /// request and confirming the response arrives within `init_timeout`.
    fn wait_for_ready(&mut self) -> Result<(), KotlinAnalysisError> {
        if !self.is_process_alive() {
            return Err(KotlinAnalysisError::SpawnFailed {
                reason: "process exited immediately after spawn".to_string(),
            });
        }

        let seq = self.next_seq();
        let request = KotlinRequest::new(seq, "ping");
        let encoded = request.encode();
        self.write_to_stdin(&encoded)?;

        self.recv_response(seq, self.config.init_timeout)?;
        Ok(())
    }

    fn is_process_alive(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }

    fn send_raw_request(&mut self, command: &str) -> Result<(), KotlinAnalysisError> {
        let seq = self.next_seq();
        let request = KotlinRequest::new(seq, command);
        let encoded = request.encode();
        self.write_to_stdin(&encoded)
    }

    fn write_to_stdin(&mut self, data: &[u8]) -> Result<(), KotlinAnalysisError> {
        let child = self
            .child
            .as_mut()
            .ok_or(KotlinAnalysisError::InvalidState {
                expected: "running",
                actual: "no child process".to_string(),
            })?;

        let stdin = child.stdin.as_mut().ok_or(KotlinAnalysisError::Io {
            source: std::io::Error::new(std::io::ErrorKind::BrokenPipe, "stdin not available"),
        })?;

        stdin.write_all(data)?;
        stdin.flush()?;
        Ok(())
    }

    fn recv_response(
        &mut self,
        request_seq: u32,
        timeout: Duration,
    ) -> Result<KotlinResponse, KotlinAnalysisError> {
        let deadline = Instant::now() + timeout;

        let buffered = std::mem::take(&mut self.peek_buffer);
        for msg in buffered {
            match msg {
                ReaderMsg::Message(m) => {
                    if let Some(resp) = self.try_match_response(&m, request_seq)? {
                        return Ok(resp);
                    }
                }
                ReaderMsg::Error(reason) => {
                    return Err(KotlinAnalysisError::Protocol { reason });
                }
            }
        }

        let rx = self
            .reader_rx
            .as_ref()
            .ok_or(KotlinAnalysisError::InvalidState {
                expected: "reader thread running",
                actual: "no reader channel".to_string(),
            })?;

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(KotlinAnalysisError::Timeout {
                    operation: format!("waiting for response to seq {request_seq}"),
                });
            }

            let msg = match rx.recv_timeout(remaining) {
                Ok(ReaderMsg::Message(m)) => m,
                Ok(ReaderMsg::Error(reason)) => {
                    return Err(KotlinAnalysisError::Protocol { reason });
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(KotlinAnalysisError::Timeout {
                        operation: format!("waiting for response to seq {request_seq}"),
                    });
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(KotlinAnalysisError::Protocol {
                        reason: "reader thread disconnected".to_string(),
                    });
                }
            };

            if let Some(resp) = self.try_match_response(&msg, request_seq)? {
                return Ok(resp);
            }
        }
    }

    fn try_match_response(
        &self,
        raw: &str,
        request_seq: u32,
    ) -> Result<Option<KotlinResponse>, KotlinAnalysisError> {
        let response: KotlinResponse =
            serde_json::from_str(raw).map_err(|e| KotlinAnalysisError::Protocol {
                reason: format!("failed to parse response: {e}"),
            })?;

        if response.msg_type == "response" && response.request_seq == Some(request_seq) {
            Ok(Some(response))
        } else {
            Ok(None)
        }
    }

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

impl KotlinRuntime for KotlinAnalysisProcess {
    fn start(&mut self) -> Result<(), KotlinAnalysisError> {
        if self.state == ProcessState::Ready {
            return Err(KotlinAnalysisError::InvalidState {
                expected: "stopped or failed",
                actual: self.state.to_string(),
            });
        }

        info!(
            java_path = %self.config.java_path.display(),
            bridge_jar = %self.config.bridge_jar_path.display(),
            working_dir = %self.config.working_dir.display(),
            "starting kotlin analysis bridge"
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
                info!("kotlin analysis bridge is ready");
                Ok(())
            }
            Err(e) => {
                error!(error = %e, "kotlin analysis bridge failed to become ready");
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

        info!("stopping kotlin analysis bridge");

        let _ = self.send_raw_request("shutdown");

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if Instant::now() >= deadline {
                warn!("kotlin analysis bridge did not exit gracefully, force killing");
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
        info!("kotlin analysis bridge stopped");
    }

    fn restart(&mut self) -> Result<(), KotlinAnalysisError> {
        if self.restart_count >= self.config.max_restarts {
            let err = KotlinAnalysisError::RestartLimitExceeded {
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
            "restarting kotlin analysis bridge"
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

        match child.try_wait() {
            Ok(Some(status)) => {
                let reason = format!("process exited with status: {status}");
                warn!(reason = %reason, "kotlin analysis bridge exited unexpectedly");
                self.state = ProcessState::Failed { reason };
                self.child = None;
                return false;
            }
            Ok(None) => {}
            Err(e) => {
                let reason = format!("failed to check process status: {e}");
                warn!(reason = %reason, "kotlin analysis bridge health check failed");
                self.state = ProcessState::Failed { reason };
                return false;
            }
        }

        if let Some(ref rx) = self.reader_rx {
            match rx.try_recv() {
                Ok(msg @ ReaderMsg::Message(_)) => {
                    self.peek_buffer.push(msg);
                }
                Ok(ReaderMsg::Error(reason)) => {
                    warn!(reason = %reason, "reader channel reported error during health check");
                    self.state = ProcessState::Failed { reason };
                    return false;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    let reason = "reader thread disconnected".to_string();
                    warn!(reason = %reason, "reader channel dead during health check");
                    self.state = ProcessState::Failed { reason };
                    return false;
                }
            }
        } else {
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
    ) -> Result<KotlinResponse, KotlinAnalysisError> {
        if self.state != ProcessState::Ready {
            return Err(KotlinAnalysisError::InvalidState {
                expected: "ready",
                actual: self.state.to_string(),
            });
        }

        let seq = self.next_seq();
        let request = match arguments {
            Some(args) => KotlinRequest::with_arguments(seq, command, args),
            None => KotlinRequest::new(seq, command),
        };

        debug!(
            seq = seq,
            command = command,
            "sending kotlin analysis request"
        );

        let encoded = request.encode();
        if let Err(e) = self.write_to_stdin(&encoded) {
            self.state = ProcessState::Failed {
                reason: e.to_string(),
            };
            return Err(e);
        }

        match self.recv_response(seq, self.config.request_timeout) {
            Ok(response) => {
                debug!(seq = seq, success = ?response.success, "received kotlin analysis response");
                Ok(response)
            }
            Err(e @ KotlinAnalysisError::Protocol { .. }) => {
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

impl Drop for KotlinAnalysisProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------
// Background reader thread
// ---------------------------------------------------------------------------

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
            let _ = tx.send(ReaderMsg::Error("unexpected end of stream".to_string()));
            return;
        }

        let trimmed = header_line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            let content_length: usize = match len_str.parse() {
                Ok(n) => n,
                Err(e) => {
                    let _ = tx.send(ReaderMsg::Error(format!("invalid Content-Length: {e}")));
                    return;
                }
            };

            let mut separator = String::new();
            if let Err(e) = reader.read_line(&mut separator) {
                let _ = tx.send(ReaderMsg::Error(format!("read error: {e}")));
                return;
            }

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
                return;
            }
            continue;
        }

        if trimmed.starts_with('{') {
            if tx.send(ReaderMsg::Message(trimmed.to_string())).is_err() {
                return;
            }
            continue;
        }

        // Unknown line format — skip (JVM may emit logging to stdout).
    }
}

// ---------------------------------------------------------------------------
// Standalone message parser for unit testing
// ---------------------------------------------------------------------------

#[cfg(test)]
fn read_message<R: BufRead>(
    reader: &mut R,
    deadline: Instant,
) -> Result<String, KotlinAnalysisError> {
    let mut header_line = String::new();
    loop {
        if Instant::now() >= deadline {
            return Err(KotlinAnalysisError::Timeout {
                operation: "reading message header".to_string(),
            });
        }

        header_line.clear();
        let bytes_read = reader
            .read_line(&mut header_line)
            .map_err(KotlinAnalysisError::from)?;
        if bytes_read == 0 {
            return Err(KotlinAnalysisError::Protocol {
                reason: "unexpected end of stream".to_string(),
            });
        }

        let trimmed = header_line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            let content_length: usize =
                len_str.parse().map_err(|e| KotlinAnalysisError::Protocol {
                    reason: format!("invalid Content-Length: {e}"),
                })?;

            let mut separator = String::new();
            reader
                .read_line(&mut separator)
                .map_err(KotlinAnalysisError::from)?;

            let mut body = vec![0u8; content_length];
            std::io::Read::read_exact(reader, &mut body).map_err(KotlinAnalysisError::from)?;

            return String::from_utf8(body).map_err(|e| KotlinAnalysisError::Protocol {
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
    use crate::runtime::KotlinRuntime;
    use std::path::PathBuf;

    fn test_config() -> KotlinAnalysisConfig {
        KotlinAnalysisConfig::new(
            PathBuf::from("/usr/bin/java"),
            PathBuf::from("/opt/bridge.jar"),
            PathBuf::from("/tmp/test-repo"),
        )
    }

    #[test]
    fn new_process_starts_in_stopped_state() {
        let proc = KotlinAnalysisProcess::new(test_config());
        assert_eq!(*proc.state(), ProcessState::Stopped);
        assert_eq!(proc.restart_count(), 0);
    }

    #[test]
    fn start_with_invalid_binary_returns_spawn_failed() {
        let config = KotlinAnalysisConfig::new(
            PathBuf::from("/nonexistent/java-that-does-not-exist"),
            PathBuf::from("/nonexistent/bridge.jar"),
            PathBuf::from("/tmp"),
        );
        let mut proc = KotlinAnalysisProcess::new(config);
        let err = proc.start().expect_err("should fail to spawn");
        assert!(
            matches!(err, KotlinAnalysisError::SpawnFailed { .. }),
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
        let mut proc = KotlinAnalysisProcess::new(config);
        proc.state = ProcessState::Ready;

        let err = proc.start().expect_err("should reject double-start");
        assert!(
            matches!(err, KotlinAnalysisError::InvalidState { .. }),
            "expected InvalidState, got: {err}"
        );
    }

    #[test]
    fn stop_from_stopped_is_idempotent() {
        let mut proc = KotlinAnalysisProcess::new(test_config());
        proc.stop();
        assert_eq!(*proc.state(), ProcessState::Stopped);
    }

    #[test]
    fn restart_limit_exceeded_returns_error() {
        let config = KotlinAnalysisConfig::new(
            PathBuf::from("/nonexistent/java"),
            PathBuf::from("/nonexistent/bridge.jar"),
            PathBuf::from("/tmp"),
        )
        .with_max_restarts(2);

        let mut proc = KotlinAnalysisProcess::new(config);
        proc.restart_count = 2;

        let err = proc.restart().expect_err("should exceed restart limit");
        assert!(
            matches!(
                err,
                KotlinAnalysisError::RestartLimitExceeded { attempts: 2 }
            ),
            "expected RestartLimitExceeded, got: {err}"
        );
        assert!(
            matches!(proc.state(), ProcessState::Failed { .. }),
            "expected failed state after restart limit exceeded"
        );
    }

    #[test]
    fn is_healthy_returns_false_when_stopped() {
        let mut proc = KotlinAnalysisProcess::new(test_config());
        assert!(!proc.is_healthy());
    }

    #[test]
    fn is_healthy_returns_false_when_failed() {
        let mut proc = KotlinAnalysisProcess::new(test_config());
        proc.state = ProcessState::Failed {
            reason: "test failure".to_string(),
        };
        assert!(!proc.is_healthy());
    }

    #[test]
    fn send_request_fails_when_not_ready() {
        let mut proc = KotlinAnalysisProcess::new(test_config());
        let err = proc
            .send_request("analyze", None)
            .expect_err("should fail when stopped");
        assert!(
            matches!(err, KotlinAnalysisError::InvalidState { .. }),
            "expected InvalidState, got: {err}"
        );
    }

    #[test]
    fn sequence_numbers_increment() {
        let proc = KotlinAnalysisProcess::new(test_config());
        let s1 = proc.next_seq();
        let s2 = proc.next_seq();
        let s3 = proc.next_seq();
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
        assert_eq!(s3, 3);
    }

    #[test]
    fn max_restarts_zero_disables_restart() {
        let config = test_config().with_max_restarts(0);
        let mut proc = KotlinAnalysisProcess::new(config);
        let err = proc.restart().expect_err("should fail immediately");
        assert!(matches!(
            err,
            KotlinAnalysisError::RestartLimitExceeded { attempts: 0 }
        ));
    }

    #[test]
    fn process_state_display() {
        assert_eq!(ProcessState::Stopped.to_string(), "stopped");
        assert_eq!(ProcessState::Ready.to_string(), "ready");
        assert_eq!(
            ProcessState::Failed {
                reason: "boom".to_string()
            }
            .to_string(),
            "failed: boom"
        );
    }

    #[test]
    fn build_command_uses_configured_binary_path() {
        let config = test_config();
        let proc = KotlinAnalysisProcess::new(config);
        let cmd = proc.build_command();
        assert_eq!(cmd.get_program(), "/usr/bin/java");
    }

    #[test]
    fn build_command_sets_working_directory() {
        let config = test_config();
        let proc = KotlinAnalysisProcess::new(config);
        let cmd = proc.build_command();
        assert_eq!(
            cmd.get_current_dir(),
            Some(PathBuf::from("/tmp/test-repo").as_path())
        );
    }

    #[test]
    fn build_command_includes_jar_argument() {
        let config = test_config();
        let proc = KotlinAnalysisProcess::new(config);
        let cmd = proc.build_command();
        let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
        assert!(args.contains(&std::ffi::OsStr::new("-jar")));
        assert!(args.contains(&std::ffi::OsStr::new("/opt/bridge.jar")));
    }

    #[test]
    fn build_command_sets_heap_limit_when_configured() {
        let config = test_config().with_heap_limit_bytes(512 * 1024 * 1024);
        let proc = KotlinAnalysisProcess::new(config);
        let cmd = proc.build_command();
        let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
        assert!(args.contains(&std::ffi::OsStr::new("-Xmx512m")));
    }

    #[test]
    fn build_command_does_not_set_heap_limit_without_config() {
        let config = test_config();
        let proc = KotlinAnalysisProcess::new(config);
        let cmd = proc.build_command();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(!args.iter().any(|a| a.starts_with("-Xmx")));
    }

    // -- Reader thread tests --

    #[test]
    fn reader_thread_sends_content_length_message() {
        let input = "Content-Length: 27\r\n\r\n{\"seq\":0,\"type\":\"response\"}";
        let (tx, rx) = mpsc::channel();
        reader_thread_main(input.as_bytes(), tx);
        match rx.recv().unwrap() {
            ReaderMsg::Message(m) => {
                assert!(m.contains("\"type\":\"response\""));
            }
            ReaderMsg::Error(e) => panic!("unexpected error: {e}"),
        }
    }

    #[test]
    fn reader_thread_sends_plain_json_line() {
        let input = "{\"seq\":0,\"type\":\"response\"}\n";
        let (tx, rx) = mpsc::channel();
        reader_thread_main(input.as_bytes(), tx);
        match rx.recv().unwrap() {
            ReaderMsg::Message(m) => {
                assert!(m.contains("\"type\":\"response\""));
            }
            ReaderMsg::Error(e) => panic!("unexpected error: {e}"),
        }
    }

    #[test]
    fn reader_thread_reports_eof_as_error() {
        let input = b"";
        let (tx, rx) = mpsc::channel();
        reader_thread_main(&input[..], tx);
        match rx.recv().unwrap() {
            ReaderMsg::Error(e) => assert!(e.contains("end of stream")),
            ReaderMsg::Message(m) => panic!("expected error, got message: {m}"),
        }
    }

    #[test]
    fn read_message_parses_content_length_framing() {
        let input = "Content-Length: 27\r\n\r\n{\"seq\":0,\"type\":\"response\"}";
        let mut reader = std::io::BufReader::new(input.as_bytes());
        let deadline = Instant::now() + Duration::from_secs(1);
        let msg = read_message(&mut reader, deadline).unwrap();
        assert!(msg.contains("\"type\":\"response\""));
    }

    #[test]
    fn read_message_parses_plain_json_line() {
        let input = "{\"seq\":0,\"type\":\"response\"}\n";
        let mut reader = std::io::BufReader::new(input.as_bytes());
        let deadline = Instant::now() + Duration::from_secs(1);
        let msg = read_message(&mut reader, deadline).unwrap();
        assert!(msg.contains("\"type\":\"response\""));
    }

    #[test]
    fn read_message_handles_eof() {
        let input = b"";
        let mut reader = std::io::BufReader::new(&input[..]);
        let deadline = Instant::now() + Duration::from_secs(1);
        let err = read_message(&mut reader, deadline).expect_err("should fail on eof");
        assert!(matches!(err, KotlinAnalysisError::Protocol { .. }));
    }

    // -- Channel-based tests --

    #[test]
    fn recv_response_times_out_on_empty_channel() {
        let mut proc = KotlinAnalysisProcess::new(test_config());
        proc.state = ProcessState::Ready;
        let (_tx, rx) = mpsc::channel::<ReaderMsg>();
        proc.reader_rx = Some(rx);

        let err = proc
            .recv_response(1, Duration::from_millis(50))
            .expect_err("should timeout");
        assert!(matches!(err, KotlinAnalysisError::Timeout { .. }));
    }

    #[test]
    fn recv_response_returns_disconnected_when_reader_gone() {
        let mut proc = KotlinAnalysisProcess::new(test_config());
        proc.state = ProcessState::Ready;
        let (tx, rx) = mpsc::channel::<ReaderMsg>();
        proc.reader_rx = Some(rx);
        drop(tx);

        let err = proc
            .recv_response(1, Duration::from_secs(1))
            .expect_err("should detect disconnect");
        assert!(matches!(err, KotlinAnalysisError::Protocol { .. }));
    }

    #[test]
    fn recv_response_surfaces_reader_error() {
        let mut proc = KotlinAnalysisProcess::new(test_config());
        proc.state = ProcessState::Ready;
        let (tx, rx) = mpsc::channel();
        proc.reader_rx = Some(rx);
        tx.send(ReaderMsg::Error("test error".to_string())).unwrap();

        let err = proc
            .recv_response(1, Duration::from_secs(1))
            .expect_err("should surface reader error");
        assert!(err.to_string().contains("test error"));
    }

    #[test]
    fn recv_response_skips_events_and_non_matching() {
        let mut proc = KotlinAnalysisProcess::new(test_config());
        proc.state = ProcessState::Ready;
        let (tx, rx) = mpsc::channel();
        proc.reader_rx = Some(rx);

        tx.send(ReaderMsg::Message(
            r#"{"seq":0,"type":"event","event":"started"}"#.to_string(),
        ))
        .unwrap();
        tx.send(ReaderMsg::Message(
            r#"{"seq":0,"type":"response","request_seq":99,"success":true}"#.to_string(),
        ))
        .unwrap();
        tx.send(ReaderMsg::Message(
            r#"{"seq":0,"type":"response","request_seq":1,"success":true,"body":{"ok":true}}"#
                .to_string(),
        ))
        .unwrap();

        let resp = proc
            .recv_response(1, Duration::from_secs(1))
            .expect("should find matching response");
        assert!(resp.is_success_for(1));
    }

    #[test]
    fn is_healthy_returns_false_with_no_reader_channel() {
        let mut proc = KotlinAnalysisProcess::new(test_config());
        proc.state = ProcessState::Ready;
        assert!(!proc.is_healthy());
        assert!(matches!(proc.state(), ProcessState::Failed { .. }));
    }

    #[test]
    fn is_healthy_detects_reader_disconnect() {
        let mut proc = KotlinAnalysisProcess::new(test_config());
        proc.state = ProcessState::Ready;
        let (tx, rx) = mpsc::channel::<ReaderMsg>();
        proc.reader_rx = Some(rx);
        // Need a child for the try_wait check not to fail.
        // Use a long-running process.
        proc.child = Some(
            Command::new("sleep")
                .arg("60")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        drop(tx);
        assert!(!proc.is_healthy());
        assert!(matches!(proc.state(), ProcessState::Failed { .. }));
    }

    #[test]
    fn is_healthy_detects_reader_error_message() {
        let mut proc = KotlinAnalysisProcess::new(test_config());
        proc.state = ProcessState::Ready;
        let (tx, rx) = mpsc::channel();
        proc.reader_rx = Some(rx);
        proc.child = Some(
            Command::new("sleep")
                .arg("60")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        tx.send(ReaderMsg::Error("pipe broken".to_string()))
            .unwrap();
        assert!(!proc.is_healthy());
    }

    #[test]
    fn spawn_with_short_lived_process_detects_exit() {
        let config = KotlinAnalysisConfig::new(
            PathBuf::from("true"), // exits immediately with 0
            PathBuf::from("/nonexistent.jar"),
            PathBuf::from("/tmp"),
        )
        .with_init_timeout(Duration::from_millis(500));

        let mut proc = KotlinAnalysisProcess::new(config);
        let err = proc.start().expect_err("should detect early exit");
        assert!(
            matches!(
                err,
                KotlinAnalysisError::SpawnFailed { .. } | KotlinAnalysisError::Protocol { .. }
            ),
            "expected SpawnFailed or Protocol, got: {err}"
        );
    }
}
