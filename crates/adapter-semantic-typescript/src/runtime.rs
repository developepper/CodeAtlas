use crate::error::TsServerError;
use crate::protocol::TsServerResponse;

/// Trait defining the runtime interface for a semantic language server process.
///
/// Abstracts the lifecycle operations (start, stop, restart, health check)
/// and request dispatch so that:
///
/// - The concrete process manager (`TsServerProcess`) can be tested behind
///   this boundary.
/// - Downstream consumers (the adapter in issue #65, merge logic in #50)
///   depend on a trait rather than a concrete type.
/// - Test doubles can implement this trait without spawning real processes.
pub trait SemanticRuntime {
    /// Starts the language server process and waits for it to become ready.
    ///
    /// After a successful return, the process must be able to handle
    /// requests. Implementations must enforce an initialization timeout
    /// and transition to a failed state if the server does not respond
    /// within that window.
    ///
    /// # Errors
    ///
    /// Returns an error if the process cannot be spawned, fails to
    /// become ready within the configured timeout, or is already running.
    fn start(&mut self) -> Result<(), TsServerError>;

    /// Stops the language server process.
    ///
    /// Attempts a graceful shutdown first, then forcefully terminates
    /// if the process does not exit within a reasonable window.
    /// Idempotent: calling stop on an already-stopped runtime is a no-op.
    fn stop(&mut self);

    /// Restarts the language server process.
    ///
    /// Stops the current process and starts a fresh one, tracking restart
    /// attempts. Implementations must enforce a maximum restart count and
    /// transition to a permanent failure state when exceeded.
    ///
    /// # Errors
    ///
    /// Returns an error if the restart limit has been reached, or if the
    /// fresh start itself fails.
    fn restart(&mut self) -> Result<(), TsServerError>;

    /// Checks whether the language server process is alive and its
    /// communication channel is intact.
    ///
    /// Returns `true` only if:
    /// - The runtime is in a ready state.
    /// - The OS process has not exited.
    /// - The protocol reader channel has not disconnected or reported errors.
    ///
    /// This is a non-blocking, best-effort check. It detects process exit
    /// and broken protocol pipes, but does **not** send a request-level
    /// probe. A process that is alive but unresponsive (hung) will not be
    /// detected until the next `send_request` call times out. Callers that
    /// need a stronger liveness guarantee should use `send_request` with a
    /// lightweight command (e.g., `configure`) as an active probe.
    ///
    /// Implementations must update their internal state to reflect any
    /// detected failures (e.g., transition to a failed state).
    fn is_healthy(&mut self) -> bool;

    /// Sends a request to the language server and waits for the matching
    /// response within the configured per-request timeout.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime is not ready, the request times out,
    /// a communication failure occurs, or the response cannot be parsed.
    fn send_request(
        &mut self,
        command: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<TsServerResponse, TsServerError>;
}
