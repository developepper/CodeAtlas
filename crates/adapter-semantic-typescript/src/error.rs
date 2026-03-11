use std::error::Error;
use std::fmt;

/// Errors specific to the tsserver lifecycle and process management.
#[derive(Debug)]
pub enum TsServerError {
    /// The tsserver process failed to start.
    SpawnFailed { reason: String },
    /// The tsserver process exited unexpectedly.
    ProcessExited { exit_code: Option<i32> },
    /// A request to tsserver timed out.
    Timeout { operation: String },
    /// The maximum number of restart attempts was exceeded.
    RestartLimitExceeded { attempts: u32 },
    /// An I/O error occurred communicating with tsserver.
    Io { source: std::io::Error },
    /// The tsserver process is not in a state that allows the requested operation.
    InvalidState {
        expected: &'static str,
        actual: String,
    },
    /// A protocol-level error in communication with tsserver.
    Protocol { reason: String },
}

impl fmt::Display for TsServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpawnFailed { reason } => {
                write!(f, "tsserver failed to start: {reason}")
            }
            Self::ProcessExited { exit_code } => {
                if let Some(code) = exit_code {
                    write!(f, "tsserver exited with code {code}")
                } else {
                    write!(f, "tsserver exited without status code")
                }
            }
            Self::Timeout { operation } => {
                write!(f, "tsserver operation timed out: {operation}")
            }
            Self::RestartLimitExceeded { attempts } => {
                write!(f, "tsserver exceeded maximum restart attempts ({attempts})")
            }
            Self::Io { source } => {
                write!(f, "tsserver I/O error: {source}")
            }
            Self::InvalidState { expected, actual } => {
                write!(
                    f,
                    "tsserver in unexpected state: expected {expected}, got {actual}"
                )
            }
            Self::Protocol { reason } => {
                write!(f, "tsserver protocol error: {reason}")
            }
        }
    }
}

impl Error for TsServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source } => Some(source),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TsServerError {
    fn from(err: std::io::Error) -> Self {
        Self::Io { source: err }
    }
}
