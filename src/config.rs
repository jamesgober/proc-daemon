//! Configuration management for the proc-daemon framework.
//!
//! This module provides a flexible configuration system that can load settings
//! from multiple sources with clear precedence rules. Built on top of figment
//! for maximum flexibility and performance.

use figment::{Figment, Provider};
use figment::providers::{Env, Format, Serialized};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{Error, Result};

/// Log level configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Trace level logging (most verbose)
    Trace,
    /// Debug level logging
    Debug,
    /// Info level logging (default)
    Info,
    /// Warning level logging
    Warn,
    /// Error level logging
    Error,
}

impl Default for LogLevel {
    fn default() -> Self {
        Self::Info
    }
}

impl From<LogLevel> for tracing::Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => Self::TRACE,
            LogLevel::Debug => Self::DEBUG,
            LogLevel::Info => Self::INFO,
            LogLevel::Warn => Self::WARN,
            LogLevel::Error => Self::ERROR,
        }
    }
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// Logging level
    pub level: LogLevel,
    /// Enable JSON formatted logs
    pub json: bool,
    /// Enable colored output (ignored for JSON logs)
    pub color: bool,
    /// Log file path (optional)
    pub file: Option<PathBuf>,
    /// Maximum log file size in bytes before rotation
    pub max_file_size: Option<u64>,
    /// Number of rotated log files to keep
    pub max_files: Option<u32>,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            json: false,
            color: true,
            file: None,
            max_file_size: Some(100 * 1024 * 1024), // 100MB
            max_files: Some(5),
        }
    }
}

/// Shutdown configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownConfig {
    /// Graceful shutdown timeout in milliseconds
    pub graceful: u64,
    /// Force shutdown timeout in milliseconds
    pub force: u64,
    /// Kill processes timeout in milliseconds
    pub kill: u64,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            graceful: crate::DEFAULT_SHUTDOWN_TIMEOUT_MS,
            force: 10_000, // 10 seconds
            kill: 15_000,  // 15 seconds
        }
    }
}

/// Performance tuning configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Number of worker threads (0 = auto-detect)
    pub worker_threads: usize,
    /// Enable thread pinning to CPU cores
    pub thread_pinning: bool,
    /// Memory pool initial size in bytes
    pub memory_pool_size: usize,
    /// Enable NUMA awareness
    pub numa_aware: bool,
    /// Enable lock-free optimizations
    pub lock_free: bool,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            worker_threads: 0, // Auto-detect
            thread_pinning: false,
            memory_pool_size: 1024 * 1024, // 1MB
            numa_aware: false,
            lock_free: true,
        }
    }
}

/// Monitoring configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable metrics collection
    pub enable_metrics: bool,
    /// Metrics collection interval in milliseconds
    pub metrics_interval_ms: u64,
    /// Enable resource usage tracking
    pub track_resources: bool,
    /// Enable health checks
    pub health_checks: bool,
    /// Health check interval in milliseconds
    pub health_check_interval_ms: u64,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enable_metrics: true,
            metrics_interval_ms: 1000, // 1 second
            track_resources: true,
            health_checks: true,
            health_check_interval_ms: 5000, // 5 seconds
        }
    }
}

/// Main daemon configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Daemon name/identifier
    pub name: String,
    /// Logging configuration
    pub logging: LogConfig,
    /// Shutdown configuration
    pub shutdown: ShutdownConfig,
    /// Performance configuration
    pub performance: PerformanceConfig,
    /// Monitoring configuration
    pub monitoring: MonitoringConfig,
    /// Working directory
    pub work_dir: Option<PathBuf>,
    /// PID file location
    pub pid_file: Option<PathBuf>,
    /// Enable configuration hot-reloading
    pub hot_reload: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // Use a static string to avoid allocation
            name: String::from("proc-daemon"),
            logging: LogConfig::default(),
            shutdown: ShutdownConfig::default(),
            performance: PerformanceConfig::default(),
            monitoring: MonitoringConfig::default(),
            work_dir: None,
            pid_file: None,
            hot_reload: false,
        }
    }
}

impl Config {
    /// Create a new config with defaults.
    ///
    /// # Errors
    ///
    /// Will return an error if the default configuration validation fails.
    pub fn new() -> Result<Self> {
        Ok(Self::default())
    }

    /// Load configuration from multiple sources with precedence:
    /// 1. Default values
    /// 2. Configuration file (if exists)
    /// 3. Environment variables
    /// 4. Provided overrides
    pub fn load() -> Result<Self> {
        Self::load_from_file(crate::DEFAULT_CONFIG_FILE)
    }

    /// Load config from a file.
    ///
    /// # Errors
    ///
    /// Will return an error if the file cannot be read or contains invalid configuration data.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        
        let mut figment = Figment::from(Serialized::defaults(Self::default()))
            .merge(Env::prefixed("DAEMON_").split("_"));

        // Add config file if it exists
        if path.exists() {
            #[cfg(feature = "toml")]
            {
                figment = figment.merge(figment::providers::Toml::file(path));
            }
            
            #[cfg(feature = "serde_json")]
            {
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    figment = figment.merge(figment::providers::Json::file(path));
                }
            }
        }

        figment.extract().map_err(Error::from)
    }

    /// Load config using a configuration provider.
    ///
    /// # Errors
    ///
    /// Will return an error if the provider fails to load a valid configuration.
    pub fn load_with_provider<P: Provider>(provider: P) -> Result<Self> {
        Figment::from(Serialized::defaults(Self::default()))
            .merge(Env::prefixed("DAEMON_").split("_"))
            .merge(provider)
            .extract()
            .map_err(Error::from)
    }

    /// Get the shutdown timeout as a Duration.
    #[must_use] pub const fn shutdown_timeout(&self) -> Duration {
        Duration::from_millis(self.shutdown.graceful)
    }

    /// Get the force shutdown timeout as a Duration.
    #[must_use] pub const fn force_shutdown_timeout(&self) -> Duration {
        Duration::from_millis(self.shutdown.force)
    }

    /// Get the kill timeout as a Duration.
    #[must_use] pub const fn kill_timeout(&self) -> Duration {
        Duration::from_millis(self.shutdown.kill)
    }

    /// Get the metrics interval as a Duration.
    #[must_use] pub const fn metrics_interval(&self) -> Duration {
        Duration::from_millis(self.monitoring.metrics_interval_ms)
    }

    /// Get the health check interval as a Duration.
    #[must_use] pub const fn health_check_interval(&self) -> Duration {
        Duration::from_millis(self.monitoring.health_check_interval_ms)
    }

    /// Validate the configuration.
    ///
    /// # Errors
    ///
    /// Will return an error if any configuration values are invalid or missing required fields.
    pub fn validate(&self) -> Result<()> {
        // Validate timeouts
        if self.shutdown.graceful == 0 {
            return Err(Error::config("Shutdown timeout must be greater than 0"));
        }

        if self.shutdown.force <= self.shutdown.graceful {
            return Err(Error::config(
                "Force timeout must be greater than graceful timeout"
            ));
        }

        if self.shutdown.kill <= self.shutdown.force {
            return Err(Error::config(
                "Kill timeout must be greater than force timeout"
            ));
        }

        // Validate performance settings
        if self.performance.memory_pool_size == 0 {
            return Err(Error::config("Memory pool size must be greater than 0"));
        }

        // Validate monitoring settings
        if self.monitoring.enable_metrics && self.monitoring.metrics_interval_ms == 0 {
            return Err(Error::config(
                "Metrics interval must be greater than 0 when metrics are enabled"
            ));
        }

        if self.monitoring.health_checks && self.monitoring.health_check_interval_ms == 0 {
            return Err(Error::config(
                "Health check interval must be greater than 0 when health checks are enabled"
            ));
        }

        // Validate name
        if self.name.is_empty() {
            return Err(Error::config("Daemon name cannot be empty"));
        }

        // Validate file paths
        if let Some(ref pid_file) = self.pid_file {
            if let Some(parent) = pid_file.parent() {
                if !parent.exists() {
                    return Err(Error::config(format!(
                        "PID file directory does not exist: {}",
                        parent.display()
                    )));
                }
            }
        }

        if let Some(ref log_file) = self.logging.file {
            if let Some(parent) = log_file.parent() {
                if !parent.exists() {
                    return Err(Error::config(format!(
                        "Log file directory does not exist: {}",
                        parent.display()
                    )));
                }
            }
        }

        Ok(())
    }

    /// Get the number of worker threads to use.
    pub fn worker_threads(&self) -> usize {
        if self.performance.worker_threads == 0 {
            std::thread::available_parallelism()
                .map(std::num::NonZeroUsize::get)
                .unwrap_or(4)
        } else {
            self.performance.worker_threads
        }
    }

    /// Check if JSON logging is enabled.
    #[must_use] pub const fn is_json_logging(&self) -> bool {
        self.logging.json
    }

    /// Check if colored logging is enabled.
    #[must_use] pub const fn is_colored_logging(&self) -> bool {
        self.logging.color && !self.logging.json
    }

    /// Create a builder for this configuration.
    #[must_use] pub fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }
}

/// Builder for creating configurations programmatically.
#[derive(Debug, Clone)]
pub struct ConfigBuilder {
    config: Config,
}

impl ConfigBuilder {
    /// Create a new configuration builder.
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }

    /// Set the daemon name.
    pub fn name<S: Into<String>>(mut self, name: S) -> Self {
        self.config.name = name.into();
        self
    }

    /// Set the log level.
    pub const fn log_level(mut self, level: LogLevel) -> Self {
        self.config.logging.level = level;
        self
    }

    /// Enable JSON logging.
    pub const fn json_logging(mut self, enabled: bool) -> Self {
        self.config.logging.json = enabled;
        self
    }

    /// Set the shutdown timeout
    ///
    /// # Errors
    /// 
    /// Will return an error if the duration exceeds u64::MAX milliseconds
    pub fn shutdown_timeout(mut self, timeout: Duration) -> Result<Self> {
        self.config.shutdown.graceful = u64::try_from(timeout.as_millis())
            .map_err(|_| Error::config("Shutdown timeout too large"))?;
        Ok(self)
    }

    /// Set the force shutdown timeout
    ///
    /// # Errors
    /// 
    /// Will return an error if the duration exceeds u64::MAX milliseconds
    pub fn force_shutdown_timeout(mut self, timeout: Duration) -> Result<Self> {
        self.config.shutdown.force = u64::try_from(timeout.as_millis())
            .map_err(|_| Error::config("Force shutdown timeout too large"))?;
        Ok(self)
    }

    /// Set the kill timeout
    ///
    /// # Errors
    /// 
    /// Will return an error if the duration exceeds u64::MAX milliseconds
    pub fn kill_timeout(mut self, timeout: Duration) -> Result<Self> {
        self.config.shutdown.kill = u64::try_from(timeout.as_millis())
            .map_err(|_| Error::config("Kill timeout too large"))?;
        Ok(self)
    }

    /// Set the working directory.
    pub fn work_dir<P: Into<PathBuf>>(mut self, dir: P) -> Self {
        self.config.work_dir = Some(dir.into());
        self
    }

    /// Set the PID file location.
    pub fn pid_file<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.config.pid_file = Some(path.into());
        self
    }

    /// Enable hot-reloading of configuration.
    pub const fn hot_reload(mut self, enabled: bool) -> Self {
        self.config.hot_reload = enabled;
        self
    }

    /// Set the number of worker threads.
    pub const fn worker_threads(mut self, threads: usize) -> Self {
        self.config.performance.worker_threads = threads;
        self
    }

    /// Enable metrics collection.
    pub const fn enable_metrics(mut self, enabled: bool) -> Self {
        self.config.monitoring.enable_metrics = enabled;
        self
    }

    /// Set the memory pool size.
    pub const fn memory_pool_size(mut self, size: usize) -> Self {
        self.config.performance.memory_pool_size = size;
        self
    }

    /// Enable lock-free optimizations.
    pub const fn lock_free(mut self, enabled: bool) -> Self {
        self.config.performance.lock_free = enabled;
        self
    }

    /// Build the configuration.
    pub fn build(self) -> Result<Config> {
        self.config.validate()?;
        Ok(self.config)
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.name, "proc-daemon");
        assert_eq!(config.logging.level, LogLevel::Info);
        assert!(!config.logging.json);
        assert!(config.logging.color);
    }

    #[test]
    fn test_config_builder() {
        let config = Config::builder()
            .name("test-daemon")
            .log_level(LogLevel::Debug)
            .json_logging(true)
            .shutdown_timeout(Duration::from_secs(10))
            .force_shutdown_timeout(Duration::from_secs(20)) // Force timeout > shutdown timeout
            .kill_timeout(Duration::from_secs(30)) // Kill timeout > force timeout
            .worker_threads(4)
            .build()
            .unwrap();

        assert_eq!(config.name, "test-daemon");
        assert_eq!(config.logging.level, LogLevel::Debug);
        assert!(config.logging.json);
        assert_eq!(config.shutdown.timeout_ms, 10_000);
        assert_eq!(config.shutdown.force_timeout_ms, 20_000);
        assert_eq!(config.shutdown.kill_timeout_ms, 30_000);
        assert_eq!(config.performance.worker_threads, 4);
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        config.shutdown.timeout_ms = 0;
        assert!(config.validate().is_err());

        config.shutdown.timeout_ms = 5000;
        config.shutdown.force_timeout_ms = 3000;
        assert!(config.validate().is_err());

        config.shutdown.force_timeout_ms = 10_000;
        config.shutdown.kill_timeout_ms = 8_000;
        assert!(config.validate().is_err());

        config.shutdown.kill_timeout_ms = 15_000;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_log_level_conversion() {
        assert_eq!(tracing::Level::from(LogLevel::Info), tracing::Level::INFO);
        assert_eq!(tracing::Level::from(LogLevel::Error), tracing::Level::ERROR);
    }

    #[test]
    fn test_duration_helpers() {
        let config = Config::default();
        assert_eq!(config.shutdown_timeout(), Duration::from_millis(5000));
        assert_eq!(config.force_shutdown_timeout(), Duration::from_millis(10_000));
        assert_eq!(config.kill_timeout(), Duration::from_millis(15_000));
    }
}
