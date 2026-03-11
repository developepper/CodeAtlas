use std::path::PathBuf;
use std::time::Duration;

/// Configuration for the tsserver process lifecycle.
#[derive(Debug, Clone)]
pub struct TsServerConfig {
    /// Path to the tsserver binary.
    ///
    /// Typically resolved from `node_modules/.bin/tsserver` or a global
    /// installation. The caller is responsible for providing a valid path.
    pub tsserver_path: PathBuf,

    /// Working directory for the tsserver process.
    ///
    /// Should be the repository root so tsserver can resolve project
    /// configuration (tsconfig.json).
    pub working_dir: PathBuf,

    /// Timeout for the tsserver process to become ready after startup.
    pub init_timeout: Duration,

    /// Timeout for individual requests sent to tsserver.
    pub request_timeout: Duration,

    /// Maximum number of automatic restart attempts before giving up.
    ///
    /// Set to 0 to disable automatic restarts.
    pub max_restarts: u32,

    /// Optional memory limit in bytes for the tsserver process.
    ///
    /// Enforced via Node.js `--max-old-space-size` flag when set.
    pub memory_limit_bytes: Option<u64>,
}

impl TsServerConfig {
    /// Creates a configuration with sensible defaults for the given paths.
    #[must_use]
    pub fn new(tsserver_path: PathBuf, working_dir: PathBuf) -> Self {
        Self {
            tsserver_path,
            working_dir,
            init_timeout: Duration::from_secs(30),
            request_timeout: Duration::from_secs(10),
            max_restarts: 3,
            memory_limit_bytes: None,
        }
    }

    /// Sets the initialization timeout.
    #[must_use]
    pub fn with_init_timeout(mut self, timeout: Duration) -> Self {
        self.init_timeout = timeout;
        self
    }

    /// Sets the per-request timeout.
    #[must_use]
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Sets the maximum restart attempts.
    #[must_use]
    pub fn with_max_restarts(mut self, max: u32) -> Self {
        self.max_restarts = max;
        self
    }

    /// Sets the memory limit in bytes.
    ///
    /// Converted to megabytes for the `--max-old-space-size` Node.js flag.
    #[must_use]
    pub fn with_memory_limit(mut self, bytes: u64) -> Self {
        self.memory_limit_bytes = Some(bytes);
        self
    }

    /// Returns the memory limit as megabytes for `--max-old-space-size`, if set.
    #[must_use]
    pub fn memory_limit_mb(&self) -> Option<u64> {
        self.memory_limit_bytes.map(|b| b / (1024 * 1024))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sensible_values() {
        let cfg = TsServerConfig::new(
            PathBuf::from("/usr/bin/tsserver"),
            PathBuf::from("/tmp/repo"),
        );
        assert_eq!(cfg.init_timeout, Duration::from_secs(30));
        assert_eq!(cfg.request_timeout, Duration::from_secs(10));
        assert_eq!(cfg.max_restarts, 3);
        assert!(cfg.memory_limit_bytes.is_none());
    }

    #[test]
    fn builder_methods_override_defaults() {
        let cfg = TsServerConfig::new(
            PathBuf::from("/usr/bin/tsserver"),
            PathBuf::from("/tmp/repo"),
        )
        .with_init_timeout(Duration::from_secs(5))
        .with_request_timeout(Duration::from_secs(2))
        .with_max_restarts(1)
        .with_memory_limit(512 * 1024 * 1024);

        assert_eq!(cfg.init_timeout, Duration::from_secs(5));
        assert_eq!(cfg.request_timeout, Duration::from_secs(2));
        assert_eq!(cfg.max_restarts, 1);
        assert_eq!(cfg.memory_limit_bytes, Some(512 * 1024 * 1024));
        assert_eq!(cfg.memory_limit_mb(), Some(512));
    }

    #[test]
    fn memory_limit_mb_rounds_down() {
        let cfg = TsServerConfig::new(
            PathBuf::from("/usr/bin/tsserver"),
            PathBuf::from("/tmp/repo"),
        )
        .with_memory_limit(300 * 1024 * 1024 + 500_000);

        // 300 MB + ~0.5 MB rounds down to 300
        assert_eq!(cfg.memory_limit_mb(), Some(300));
    }

    #[test]
    fn memory_limit_mb_none_when_unset() {
        let cfg = TsServerConfig::new(
            PathBuf::from("/usr/bin/tsserver"),
            PathBuf::from("/tmp/repo"),
        );
        assert_eq!(cfg.memory_limit_mb(), None);
    }
}
