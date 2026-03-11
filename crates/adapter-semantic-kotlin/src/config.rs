use std::path::PathBuf;
use std::time::Duration;

/// Configuration for the Kotlin analysis bridge subprocess.
///
/// Controls binary paths, timeouts, memory limits, and restart behaviour.
pub struct KotlinAnalysisConfig {
    /// Path to the `java` binary.
    pub java_path: PathBuf,
    /// Path to the analysis bridge JAR.
    pub bridge_jar_path: PathBuf,
    /// Working directory for the JVM process (repository root).
    pub working_dir: PathBuf,
    /// Maximum time to wait for the JVM to start and become ready.
    /// Default: 45 seconds (JVM cold start is slower than Node.js).
    pub init_timeout: Duration,
    /// Maximum time to wait for a single analysis request to complete.
    /// Default: 10 seconds.
    pub request_timeout: Duration,
    /// Maximum number of process restarts before entering a permanent
    /// failure state. Default: 3.
    pub max_restarts: u32,
    /// Optional JVM heap size limit in bytes. When set, the process is
    /// started with `-Xmx{megabytes}m`.
    pub heap_limit_bytes: Option<u64>,
}

impl KotlinAnalysisConfig {
    /// Creates a new config with reasonable defaults.
    pub fn new(java_path: PathBuf, bridge_jar_path: PathBuf, working_dir: PathBuf) -> Self {
        Self {
            java_path,
            bridge_jar_path,
            working_dir,
            init_timeout: Duration::from_secs(45),
            request_timeout: Duration::from_secs(10),
            max_restarts: 3,
            heap_limit_bytes: None,
        }
    }

    /// Returns the heap limit in megabytes, if configured.
    #[must_use]
    pub fn heap_limit_mb(&self) -> Option<u64> {
        self.heap_limit_bytes.map(|bytes| bytes / (1024 * 1024))
    }

    /// Sets the init timeout.
    #[must_use]
    pub fn with_init_timeout(mut self, timeout: Duration) -> Self {
        self.init_timeout = timeout;
        self
    }

    /// Sets the request timeout.
    #[must_use]
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Sets the maximum restart count.
    #[must_use]
    pub fn with_max_restarts(mut self, max: u32) -> Self {
        self.max_restarts = max;
        self
    }

    /// Sets the JVM heap limit in bytes.
    #[must_use]
    pub fn with_heap_limit_bytes(mut self, bytes: u64) -> Self {
        self.heap_limit_bytes = Some(bytes);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sensible_values() {
        let config = KotlinAnalysisConfig::new(
            PathBuf::from("/usr/bin/java"),
            PathBuf::from("/opt/bridge.jar"),
            PathBuf::from("/tmp/repo"),
        );
        assert_eq!(config.init_timeout, Duration::from_secs(45));
        assert_eq!(config.request_timeout, Duration::from_secs(10));
        assert_eq!(config.max_restarts, 3);
        assert!(config.heap_limit_bytes.is_none());
    }

    #[test]
    fn builder_methods_override_defaults() {
        let config = KotlinAnalysisConfig::new(
            PathBuf::from("/usr/bin/java"),
            PathBuf::from("/opt/bridge.jar"),
            PathBuf::from("/tmp/repo"),
        )
        .with_init_timeout(Duration::from_secs(60))
        .with_request_timeout(Duration::from_secs(20))
        .with_max_restarts(5)
        .with_heap_limit_bytes(512 * 1024 * 1024);

        assert_eq!(config.init_timeout, Duration::from_secs(60));
        assert_eq!(config.request_timeout, Duration::from_secs(20));
        assert_eq!(config.max_restarts, 5);
        assert_eq!(config.heap_limit_mb(), Some(512));
    }

    #[test]
    fn heap_limit_mb_none_when_unset() {
        let config = KotlinAnalysisConfig::new(
            PathBuf::from("/usr/bin/java"),
            PathBuf::from("/opt/bridge.jar"),
            PathBuf::from("/tmp/repo"),
        );
        assert_eq!(config.heap_limit_mb(), None);
    }

    #[test]
    fn heap_limit_mb_rounds_down() {
        let config = KotlinAnalysisConfig::new(
            PathBuf::from("/usr/bin/java"),
            PathBuf::from("/opt/bridge.jar"),
            PathBuf::from("/tmp/repo"),
        )
        .with_heap_limit_bytes(500 * 1024 * 1024 + 999);
        assert_eq!(config.heap_limit_mb(), Some(500));
    }
}
