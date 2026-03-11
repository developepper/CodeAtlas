use std::error::Error;
use std::fmt;

/// Errors specific to the Kotlin analysis bridge lifecycle and process management.
#[derive(Debug)]
pub enum KotlinAnalysisError {
    /// The analysis bridge process failed to start.
    SpawnFailed { reason: String },
    /// The analysis bridge process exited unexpectedly.
    ProcessExited { exit_code: Option<i32> },
    /// A request to the analysis bridge timed out.
    Timeout { operation: String },
    /// The maximum number of restart attempts was exceeded.
    RestartLimitExceeded { attempts: u32 },
    /// An I/O error occurred communicating with the analysis bridge.
    Io { source: std::io::Error },
    /// The analysis bridge is not in a state that allows the requested operation.
    InvalidState {
        expected: &'static str,
        actual: String,
    },
    /// A protocol-level error in communication with the analysis bridge.
    Protocol { reason: String },
}

impl fmt::Display for KotlinAnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpawnFailed { reason } => {
                write!(f, "kotlin analysis bridge failed to start: {reason}")
            }
            Self::ProcessExited { exit_code } => {
                if let Some(code) = exit_code {
                    write!(f, "kotlin analysis bridge exited with code {code}")
                } else {
                    write!(f, "kotlin analysis bridge exited without status code")
                }
            }
            Self::Timeout { operation } => {
                write!(f, "kotlin analysis bridge operation timed out: {operation}")
            }
            Self::RestartLimitExceeded { attempts } => {
                write!(
                    f,
                    "kotlin analysis bridge exceeded maximum restart attempts ({attempts})"
                )
            }
            Self::Io { source } => {
                write!(f, "kotlin analysis bridge I/O error: {source}")
            }
            Self::InvalidState { expected, actual } => {
                write!(
                    f,
                    "kotlin analysis bridge in unexpected state: expected {expected}, got {actual}"
                )
            }
            Self::Protocol { reason } => {
                write!(f, "kotlin analysis bridge protocol error: {reason}")
            }
        }
    }
}

impl Error for KotlinAnalysisError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source } => Some(source),
            _ => None,
        }
    }
}

impl From<std::io::Error> for KotlinAnalysisError {
    fn from(err: std::io::Error) -> Self {
        Self::Io { source: err }
    }
}
