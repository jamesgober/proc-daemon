//! Core daemon implementation with builder pattern.
//!
//! This module provides the main `Daemon` struct and `DaemonBuilder` for creating
//! high-performance, resilient daemon services. The builder pattern allows for
//! flexible configuration while maintaining zero-copy performance characteristics.

use std::future::Future;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, instrument, warn};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::shutdown::{ShutdownCoordinator, ShutdownReason};
use crate::signal::{ConfigurableSignalHandler, SignalConfig, SignalHandler};
use crate::subsystem::{Subsystem, SubsystemId, SubsystemManager};

#[cfg(feature = "config-watch")]
use arc_swap::ArcSwap;
#[cfg(feature = "config-watch")]
use notify::RecommendedWatcher;

/// Type alias for subsystem registration function
type SubsystemRegistrationFn = Box<dyn FnOnce(&SubsystemManager) -> SubsystemId + Send + 'static>;

/// Main daemon instance that coordinates all subsystems and handles lifecycle.
pub struct Daemon {
    /// Configuration
    config: Arc<Config>,
    /// Live-updating configuration snapshot (when config-watch is enabled)
    #[cfg(feature = "config-watch")]
    config_shared: Arc<ArcSwap<Config>>,
    /// Shutdown coordination
    shutdown_coordinator: ShutdownCoordinator,
    /// Subsystem management
    subsystem_manager: SubsystemManager,
    /// Signal handling
    signal_handler: Option<SignalHandlerKind>,
    /// Keep the config watcher alive (when enabled)
    #[cfg(feature = "config-watch")]
    _config_watcher: Option<RecommendedWatcher>,
    /// Start time
    started_at: Option<Instant>,
}

struct PidFileGuard {
    path: PathBuf,
}

impl PidFileGuard {
    fn create(path: &Path) -> Result<Self> {
        let pid = std::process::id();
        let mut file = std::fs::File::create(path).map_err(|e| {
            Error::io_with_source(
                format!("Failed to create PID file at {}", path.display()),
                e,
            )
        })?;
        writeln!(file, "{pid}").map_err(|e| {
            Error::io_with_source(format!("Failed to write PID file at {}", path.display()), e)
        })?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

struct RotatingFileWriterInner {
    file: std::fs::File,
    path: PathBuf,
    max_size: u64,
    max_files: u32,
    size: u64,
}

#[derive(Clone)]
struct RotatingFileWriter {
    inner: Arc<std::sync::Mutex<RotatingFileWriterInner>>,
}

impl RotatingFileWriter {
    fn new(path: PathBuf, max_size: Option<u64>, max_files: Option<u32>) -> io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);

        let max_size = max_size.unwrap_or(u64::MAX);
        let max_files = max_files.unwrap_or(0);

        Ok(Self {
            inner: Arc::new(std::sync::Mutex::new(RotatingFileWriterInner {
                file,
                path,
                max_size,
                max_files,
                size,
            })),
        })
    }

    fn rotate_locked(inner: &mut RotatingFileWriterInner) -> io::Result<()> {
        if inner.max_files == 0 {
            return Ok(());
        }

        for idx in (1..=inner.max_files).rev() {
            let from = Self::rotated_path(&inner.path, idx - 1);
            let to = Self::rotated_path(&inner.path, idx);
            if from.exists() {
                let _ = std::fs::remove_file(&to);
                std::fs::rename(&from, &to)?;
            }
        }
        inner.file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&inner.path)?;
        inner.size = 0;
        Ok(())
    }

    fn rotated_path(path: &Path, idx: u32) -> PathBuf {
        if idx == 0 {
            return path.to_path_buf();
        }
        PathBuf::from(format!("{}.{}", path.display(), idx))
    }
}

struct RotatingFileWriterGuard {
    inner: Arc<std::sync::Mutex<RotatingFileWriterInner>>,
}

impl io::Write for RotatingFileWriterGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("rotating log writer mutex poisoned"))?;

        if inner.size.saturating_add(buf.len() as u64) > inner.max_size {
            RotatingFileWriter::rotate_locked(&mut inner)?;
        }

        let written = inner.file.write(buf)?;
        inner.size = inner.size.saturating_add(written as u64);
        drop(inner);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("rotating log writer mutex poisoned"))?;
        let result = inner.file.flush();
        drop(inner);
        result
    }
}

impl<'a> tracing_subscriber::fmt::writer::MakeWriter<'a> for RotatingFileWriter {
    type Writer = RotatingFileWriterGuard;

    fn make_writer(&'a self) -> Self::Writer {
        RotatingFileWriterGuard {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[derive(Clone)]
enum SignalHandlerKind {
    Default(Arc<SignalHandler>),
    Configurable(Arc<ConfigurableSignalHandler>),
}

impl SignalHandlerKind {
    async fn handle_signals(&self) -> Result<()> {
        match self {
            Self::Default(handler) => handler.handle_signals().await,
            Self::Configurable(handler) => handler.handle_signals().await,
        }
    }

    fn stop(&self) {
        match self {
            Self::Default(handler) => handler.stop(),
            Self::Configurable(handler) => handler.stop(),
        }
    }
}

impl Daemon {
    /// Create a new daemon builder with the provided configuration.
    #[must_use]
    pub fn builder(config: Config) -> DaemonBuilder {
        DaemonBuilder::new(config)
    }

    /// Create a new daemon with default configuration.
    ///
    /// # Errors
    ///
    /// Will return an error if the default configuration is invalid.
    pub fn with_defaults() -> Result<DaemonBuilder> {
        let config = Config::new()?;
        Ok(Self::builder(config))
    }

    /// Run the daemon until shutdown is requested.
    /// This is the main entry point that starts all subsystems and waits for shutdown.
    ///
    /// # Errors
    ///
    /// Returns an error if logging initialization fails, configuration validation fails,
    /// subsystem startup fails, or if there is an error during the shutdown sequence.
    #[instrument(skip(self), fields(daemon_name = %self.config.name))]
    pub async fn run(mut self) -> Result<()> {
        info!(daemon_name = %self.config.name, "Starting daemon");
        self.started_at = Some(Instant::now());

        // Apply working directory before initializing logging or PID files
        if let Some(work_dir) = &self.config.work_dir {
            std::env::set_current_dir(work_dir).map_err(|e| {
                Error::io_with_source(
                    format!("Failed to set working directory to {}", work_dir.display()),
                    e,
                )
            })?;
        }

        // Write PID file if configured
        let _pid_guard = if let Some(pid_file) = &self.config.pid_file {
            Some(PidFileGuard::create(pid_file)?)
        } else {
            None
        };

        // Initialize logging
        self.init_logging()?;

        // Apply optional scheduler hints (no-op placeholders for future tuning)
        #[cfg(feature = "scheduler-hints")]
        {
            crate::scheduler::apply_process_hints(&self.config);
            crate::scheduler::apply_runtime_hints();
        }

        // Start all subsystems
        if let Err(e) = self.subsystem_manager.start_all().await {
            error!(error = %e, "Failed to start all subsystems");
            return Err(e);
        }

        // Start signal handling in the background
        // Only spawn when a supported async runtime is enabled.
        #[cfg(any(feature = "tokio", feature = "async-std"))]
        let signal_task = self.signal_handler.as_ref().map(|signal_handler| {
            let handler = signal_handler.clone();
            Self::spawn_signal_handler(handler)
        });

        #[cfg(not(any(feature = "tokio", feature = "async-std")))]
        let _signal_task: Option<()> = None;

        // Wait for shutdown to be initiated
        info!("Daemon started successfully, waiting for shutdown signal");

        // Main daemon loop - wait for shutdown
        loop {
            if self.shutdown_coordinator.is_shutdown() {
                break;
            }

            // Check subsystem health periodically
            if self.config.monitoring.health_checks {
                let health_results = self.subsystem_manager.run_health_checks();
                // Early exit on first unhealthy subsystem to avoid allocations
                let mut found_unhealthy = false;
                for (id, name, healthy) in &health_results {
                    if !healthy {
                        if !found_unhealthy {
                            warn!("Unhealthy subsystems detected:");
                            found_unhealthy = true;
                        }
                        warn!(subsystem_id = id, subsystem_name = %name, "Unhealthy subsystem");
                        // Could implement auto-restart logic here
                    }
                }
            }

            // Sleep for a short interval
            #[cfg(feature = "tokio")]
            tokio::time::sleep(self.config.health_check_interval()).await;

            #[cfg(all(feature = "async-std", not(feature = "tokio")))]
            async_std::task::sleep(self.config.health_check_interval()).await;

            #[cfg(not(any(feature = "tokio", feature = "async-std")))]
            std::thread::sleep(self.config.health_check_interval());
        }

        // Graceful shutdown sequence
        info!("Shutdown initiated, beginning graceful shutdown");

        // Stop signal handling
        if let Some(signal_handler) = &self.signal_handler {
            signal_handler.stop();
        }

        // Wait for signal handler task to complete
        #[cfg(any(feature = "tokio", feature = "async-std"))]
        if let Some(task) = signal_task {
            #[cfg(feature = "tokio")]
            {
                if let Err(e) = task.await {
                    warn!(error = %e, "Signal handler task failed");
                }
            }

            #[cfg(all(feature = "async-std", not(feature = "tokio")))]
            {
                if let Err(e) = task.await {
                    warn!(error = %e, "Signal handler task failed");
                }
            }
        }

        // Stop all subsystems
        if let Err(e) = self.subsystem_manager.stop_all().await {
            error!(error = %e, "Failed to stop all subsystems gracefully");
        }

        // Wait for graceful shutdown with timeout
        if let Err(e) = self.shutdown_coordinator.wait_for_shutdown().await {
            warn!(error = %e, "Graceful shutdown timeout exceeded");

            // Wait for force shutdown timeout
            if let Err(e) = self.shutdown_coordinator.wait_for_force_shutdown().await {
                error!(error = %e, "Force shutdown timeout exceeded");
            }

            // Wait for kill shutdown timeout
            if let Err(e) = self.shutdown_coordinator.wait_for_kill_shutdown().await {
                error!(error = %e, "Kill shutdown timeout exceeded, exiting immediately");
            }
        }

        let elapsed = self.started_at.map(|t| t.elapsed());
        info!(uptime = ?elapsed, "Daemon shutdown complete");

        Ok(())
    }

    /// Initialize the logging system based on configuration.
    fn init_logging(&self) -> Result<()> {
        use tracing_subscriber::fmt::format::FmtSpan;
        use tracing_subscriber::fmt::writer::BoxMakeWriter;
        use tracing_subscriber::{EnvFilter, FmtSubscriber};

        let level: tracing::Level = self.config.logging.level.into();
        let filter = EnvFilter::from_default_env().add_directive(level.into());

        // Configure output writer
        let writer = if let Some(path) = &self.config.logging.file {
            Some(
                RotatingFileWriter::new(
                    path.clone(),
                    self.config.logging.max_file_size,
                    self.config.logging.max_files,
                )
                .map_err(|e| {
                    Error::io_with_source(
                        format!("Failed to initialize log file at {}", path.display()),
                        e,
                    )
                })?,
            )
        } else {
            None
        };

        let make_writer = |writer: &Option<RotatingFileWriter>| {
            writer.as_ref().map_or_else(
                || BoxMakeWriter::new(std::io::stdout),
                |writer| BoxMakeWriter::new(writer.clone()),
            )
        };

        // Configure output format
        if self.config.is_json_logging() {
            #[cfg(feature = "json-logs")]
            {
                let base_subscriber = FmtSubscriber::builder()
                    .with_env_filter(filter)
                    .with_span_events(FmtSpan::CLOSE)
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_thread_names(true)
                    .with_writer(make_writer(&writer));

                let json_subscriber = base_subscriber
                    .json()
                    .flatten_event(true)
                    .with_current_span(false);

                tracing::subscriber::set_global_default(json_subscriber.finish()).map_err(|e| {
                    Error::config(format!("Failed to initialize JSON logging: {e}"))
                })?;

                return Ok(()); // Return early as logging is initialized
            }

            #[cfg(not(feature = "json-logs"))]
            {
                return Err(Error::config(
                    "JSON logging requested but feature not enabled",
                ));
            }
        }

        // Regular non-JSON logging
        let base_subscriber = FmtSubscriber::builder()
            .with_env_filter(filter)
            .with_span_events(FmtSpan::CLOSE)
            .with_target(true)
            .with_thread_ids(true)
            .with_thread_names(true)
            .with_writer(make_writer(&writer));

        let regular_subscriber = base_subscriber
            .with_ansi(self.config.is_colored_logging())
            .compact();

        tracing::subscriber::set_global_default(regular_subscriber.finish())
            .map_err(|e| Error::config(format!("Failed to initialize logging: {e}")))?;

        debug!(
            "Logging initialized with level: {:?}",
            self.config.logging.level
        );
        Ok(())
    }

    /// Spawn the signal handler task.
    #[cfg(feature = "tokio")]
    fn spawn_signal_handler(handler: SignalHandlerKind) -> tokio::task::JoinHandle<Result<()>> {
        tokio::spawn(async move { handler.handle_signals().await })
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    fn spawn_signal_handler(handler: SignalHandlerKind) -> async_std::task::JoinHandle<Result<()>> {
        async_std::task::spawn(async move { handler.handle_signals().await })
    }

    /// Get daemon statistics.
    pub fn get_stats(&self) -> DaemonStats {
        let subsystem_stats = self.subsystem_manager.get_stats();
        let shutdown_stats = self.shutdown_coordinator.get_stats();
        let total_restarts = subsystem_stats.total_restarts;

        DaemonStats {
            name: self.config.name.clone(),
            uptime: self.started_at.map(|t| t.elapsed()),
            is_shutdown: shutdown_stats.is_shutdown,
            shutdown_reason: shutdown_stats.reason,
            subsystem_stats,
            total_restarts,
        }
    }

    /// Request graceful shutdown programmatically.
    pub fn shutdown(&self) -> bool {
        self.shutdown_coordinator
            .initiate_shutdown(ShutdownReason::Requested)
    }

    /// Check if the daemon is running.
    pub fn is_running(&self) -> bool {
        !self.shutdown_coordinator.is_shutdown()
    }

    /// Get the daemon configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get a snapshot of the current configuration. When the `config-watch` feature
    /// is enabled and hot-reload is active, this reflects the most recent loaded
    /// configuration; otherwise it returns the initial configuration.
    #[cfg(feature = "config-watch")]
    pub fn config_snapshot(&self) -> Arc<Config> {
        self.config_shared.load_full()
    }
}

impl Clone for Daemon {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            #[cfg(feature = "config-watch")]
            config_shared: Arc::clone(&self.config_shared),
            shutdown_coordinator: self.shutdown_coordinator.clone(),
            subsystem_manager: self.subsystem_manager.clone(),
            signal_handler: self.signal_handler.clone(),
            #[cfg(feature = "config-watch")]
            _config_watcher: None,
            started_at: self.started_at,
        }
    }
}

/// Statistics about the daemon's current state.
#[derive(Debug, Clone)]
pub struct DaemonStats {
    /// Daemon name
    pub name: String,
    /// Time since daemon started
    pub uptime: Option<std::time::Duration>,
    /// Whether shutdown has been initiated
    pub is_shutdown: bool,
    /// Reason for shutdown (if any)
    pub shutdown_reason: Option<crate::shutdown::ShutdownReason>,
    /// Subsystem statistics
    pub subsystem_stats: crate::subsystem::SubsystemStats,
    /// Total number of subsystem restarts
    pub total_restarts: u64,
}

/// Builder for creating daemon instances with fluent API.
pub struct DaemonBuilder {
    config: Config,
    // Pre-allocate the vector with a reasonable capacity
    subsystems: Vec<SubsystemRegistrationFn>,
    signal_config: Option<SignalConfig>,
    enable_signals: bool,
    config_path: Option<PathBuf>,
}

impl DaemonBuilder {
    /// Create a new daemon builder with the provided configuration.
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self {
            config,
            // Pre-allocate the subsystems vector with a reasonable capacity
            subsystems: Vec::with_capacity(16),
            signal_config: None,
            enable_signals: true,
            config_path: None,
        }
    }

    /// Configure signal handling.
    #[must_use]
    pub fn with_signal_config(mut self, config: SignalConfig) -> Self {
        self.signal_config = Some(config);
        self
    }

    /// Override the configuration file path used for hot-reload.
    #[must_use]
    pub fn with_config_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.config_path = Some(path.into());
        self
    }

    /// Disable signal handling.
    #[must_use]
    pub const fn without_signals(mut self) -> Self {
        self.enable_signals = false;
        self
    }

    /// Enable only specific signals.
    #[must_use]
    pub fn with_signals(mut self, sigterm: bool, sigint: bool) -> Self {
        let mut config = SignalConfig::new();
        if !sigterm {
            config = config.without_sigterm();
        }
        if !sigint {
            config = config.without_sigint();
        }
        self.signal_config = Some(config);
        self
    }

    /// Add a task that will be run as part of the daemon.
    ///
    /// A task is a function that will be executed repeatedly until shutdown is requested.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the task for identification
    /// * `task_fn` - Function that implements the task logic
    ///
    /// # Returns
    ///
    /// Updated builder instance
    #[must_use]
    pub fn with_task<F, Fut>(mut self, name: &str, task_fn: F) -> Self
    where
        F: Fn(crate::shutdown::ShutdownHandle) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        // Clone the name to avoid lifetime issues
        let name = name.to_string();
        let subsystem_fn = Box::new(move |subsystem_manager: &SubsystemManager| {
            subsystem_manager.register_fn(&name, task_fn)
        });

        self.subsystems.push(subsystem_fn);
        self
    }

    /// Add a subsystem that will be managed by the daemon.
    ///
    /// A subsystem is a component that implements the `Subsystem` trait.
    ///
    /// # Arguments
    ///
    /// * `subsystem` - The subsystem to add
    ///
    /// # Returns
    ///
    /// Updated builder instance
    #[must_use]
    pub fn with_subsystem<S>(mut self, subsystem: S) -> Self
    where
        S: Subsystem + Send + Sync + 'static,
    {
        let subsystem_fn = Box::new(move |subsystem_manager: &SubsystemManager| {
            subsystem_manager.register(subsystem)
        });

        self.subsystems.push(subsystem_fn);
        self
    }

    /// Add a subsystem using a registration function.
    ///
    /// This is a lower-level method that gives direct access to the `SubsystemManager`
    /// for registration. It's useful when you need more control over the registration process.
    ///
    /// # Arguments
    ///
    /// * `name` - Name for identification in logs
    /// * `register_fn` - Function that handles the subsystem registration
    ///
    /// # Returns
    ///
    /// Updated builder instance
    #[must_use]
    pub fn with_subsystem_fn<F>(mut self, name: &str, register_fn: F) -> Self
    where
        F: FnOnce(&SubsystemManager) -> SubsystemId + Send + 'static,
    {
        debug!("Adding subsystem registration function for {}", name);
        self.subsystems.push(Box::new(register_fn));
        self
    }

    /// Build the daemon instance.
    /// Builds a daemon from the configured builder
    ///
    /// # Errors
    ///
    /// Returns an error if the daemon configuration is invalid or if required components cannot be initialized
    pub fn build(self) -> Result<Daemon> {
        // Validate configuration
        self.config.validate()?;

        // Create shutdown coordinator
        let shutdown_coordinator = ShutdownCoordinator::new(
            self.config.shutdown.graceful,
            self.config.shutdown.force,
            self.config.shutdown.kill,
        );

        // Create subsystem manager
        let subsystem_manager = SubsystemManager::new(shutdown_coordinator.clone());

        // Register all subsystems
        for subsystem_fn in self.subsystems {
            let id = subsystem_fn(&subsystem_manager);
            debug!(subsystem_id = id, "Registered subsystem");
        }

        // Create signal handler if enabled
        let signal_handler = if self.enable_signals {
            self.signal_config.clone().map_or_else(
                || {
                    Some(SignalHandlerKind::Default(Arc::new(SignalHandler::new(
                        shutdown_coordinator.clone(),
                    ))))
                },
                |config| {
                    Some(SignalHandlerKind::Configurable(Arc::new(
                        ConfigurableSignalHandler::new(shutdown_coordinator.clone(), config),
                    )))
                },
            )
        } else {
            None
        };

        // Prepare configuration arcs
        let config_arc = Arc::new(self.config);

        #[cfg(feature = "config-watch")]
        let config_shared: Arc<ArcSwap<Config>> = Arc::new(ArcSwap::from(config_arc.clone()));

        // Optionally start config watcher when hot_reload is enabled
        #[cfg(feature = "config-watch")]
        let mut config_watcher: Option<RecommendedWatcher> = None;

        #[cfg(feature = "config-watch")]
        {
            if config_arc.hot_reload {
                let swap = Arc::clone(&config_shared);
                let watch_path = self
                    .config_path
                    .or_else(|| {
                        config_arc
                            .work_dir
                            .as_ref()
                            .map(|d| d.join(crate::DEFAULT_CONFIG_FILE))
                    })
                    .unwrap_or_else(|| PathBuf::from(crate::DEFAULT_CONFIG_FILE));

                let watch_path_for_cb = watch_path.clone();

                match Config::watch_file(&watch_path, move |res| match res {
                    Ok(new_cfg) => {
                        swap.store(Arc::new(new_cfg));
                        info!(
                            "Configuration hot-reloaded from {}",
                            watch_path_for_cb.display()
                        );
                    }
                    Err(e) => {
                        warn!(error = %e, "Configuration reload failed");
                    }
                }) {
                    Ok(w) => {
                        config_watcher = Some(w);
                        info!("Config watcher started for {}", watch_path.display());
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to start config watcher; continuing without hot-reload");
                    }
                }
            }
        }

        Ok(Daemon {
            config: config_arc,
            #[cfg(feature = "config-watch")]
            config_shared,
            shutdown_coordinator,
            subsystem_manager,
            signal_handler,
            #[cfg(feature = "config-watch")]
            _config_watcher: config_watcher,
            started_at: None,
        })
    }

    /// Build and run the daemon in one step.
    /// Runs the daemon until completion or error
    ///
    /// # Errors
    ///
    /// Returns an error if the daemon encounters an unrecoverable error during execution
    pub async fn run(self) -> Result<()> {
        let daemon = self.build()?;
        daemon.run().await
    }
}

/// Convenience macro for creating subsystems from closures.
#[macro_export]
macro_rules! subsystem {
    ($name:expr, $closure:expr) => {
        Box::new(move |shutdown: $crate::shutdown::ShutdownHandle| {
            Box::pin($closure(shutdown)) as Pin<Box<dyn Future<Output = $crate::Result<()>> + Send>>
        })
    };
}

/// Convenience macro for creating simple task-based subsystems.
#[macro_export]
macro_rules! task {
    ($name:expr, $body:expr) => {
        |shutdown: $crate::shutdown::ShutdownHandle| async move {
            #[cfg(feature = "tokio")]
            let mut shutdown = shutdown;
            loop {
                #[cfg(feature = "tokio")]
                {
                    tokio::select! {
                        _ = shutdown.cancelled() => {
                            tracing::info!("Task '{}' shutting down", $name);
                            break;
                        }
                        _ = async { $body } => {}
                    }
                }

                #[cfg(all(feature = "async-std", not(feature = "tokio")))]
                {
                    if shutdown.is_shutdown() {
                        tracing::info!("Task '{}' shutting down", $name);
                        break;
                    }
                    // Execute the body directly without awaiting
                    $body;
                    // Add a small delay to prevent tight loop
                    async_std::task::sleep(std::time::Duration::from_millis(10)).await;
                }
            }
            Ok(())
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::time::Duration;

    async fn test_subsystem(shutdown: crate::shutdown::ShutdownHandle) -> Result<()> {
        #[cfg(feature = "tokio")]
        let mut shutdown = shutdown;
        loop {
            #[cfg(feature = "tokio")]
            {
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    () = tokio::time::sleep(Duration::from_millis(10)) => {}
                }
            }

            #[cfg(all(feature = "async-std", not(feature = "tokio")))]
            {
                if shutdown.is_shutdown() {
                    break;
                }
                async_std::task::sleep(Duration::from_millis(10)).await;
            }
        }
        Ok(())
    }

    #[cfg(feature = "tokio")]
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_daemon_builder() {
        // Add a test timeout to prevent freezing
        let test_result = tokio::time::timeout(Duration::from_secs(5), async {
            let config = Config::new().unwrap();
            let daemon = Daemon::builder(config)
                .with_subsystem_fn("test", |subsystem_manager| {
                    subsystem_manager.register_fn("test_subsystem", test_subsystem)
                })
                .build()
                .unwrap();

            assert!(daemon.is_running());
            assert_eq!(daemon.config().name, "proc-daemon");

            // Ensure proper cleanup
            daemon.shutdown();
        })
        .await;

        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    #[async_std::test]
    async fn test_daemon_builder() {
        // Add a test timeout to prevent freezing
        let test_result = async_std::future::timeout(Duration::from_secs(5), async {
            let config = Config::new().unwrap();
            let daemon = Daemon::builder(config)
                .with_subsystem_fn("test", |subsystem_manager| {
                    subsystem_manager.register_fn("test_subsystem", test_subsystem)
                })
                .build()
                .unwrap();

            assert!(daemon.is_running());
            assert_eq!(daemon.config().name, "proc-daemon");

            // Ensure proper cleanup
            daemon.shutdown();
        })
        .await;

        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[cfg(feature = "tokio")]
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_daemon_with_defaults() {
        // Add a test timeout to prevent freezing
        let test_result = tokio::time::timeout(Duration::from_secs(5), async {
            let builder = Daemon::with_defaults().unwrap();
            let daemon = builder
                .with_task("simple_task", |_shutdown| async {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Ok(())
                })
                .build()
                .unwrap();

            assert!(daemon.is_running());

            // Ensure proper cleanup
            daemon.shutdown();
        })
        .await;

        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    #[async_std::test]
    async fn test_daemon_with_defaults() {
        // Add a test timeout to prevent freezing
        let test_result = async_std::future::timeout(Duration::from_secs(5), async {
            let builder = Daemon::with_defaults().unwrap();
            let daemon = builder
                .with_task("simple_task", |_shutdown| async {
                    async_std::task::sleep(Duration::from_millis(10)).await;
                    Ok(())
                })
                .build()
                .unwrap();

            assert!(daemon.is_running());

            // Ensure proper cleanup
            daemon.shutdown();
        })
        .await;

        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[cfg(feature = "tokio")]
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_daemon_shutdown() {
        // Add a test timeout to prevent freezing
        let test_result = tokio::time::timeout(Duration::from_secs(5), async {
            let config = Config::builder()
                .name("test-daemon")
                .shutdown_timeout(Duration::from_millis(100))
                .unwrap()
                .build()
                .unwrap();

            let daemon = Daemon::builder(config)
                .with_subsystem_fn("test", |subsystem_manager| {
                    subsystem_manager.register_fn("test_subsystem", test_subsystem)
                })
                .without_signals()
                .build()
                .unwrap();

            // Request shutdown
            daemon.shutdown();
            assert!(!daemon.is_running());
        })
        .await;

        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    #[async_std::test]
    async fn test_daemon_shutdown() {
        // Add a test timeout to prevent freezing
        let test_result = async_std::future::timeout(Duration::from_secs(5), async {
            let config = Config::builder()
                .name("test-daemon")
                .shutdown_timeout(Duration::from_millis(100))
                .unwrap()
                .build()
                .unwrap();

            let daemon = Daemon::builder(config)
                .with_subsystem_fn("test", |subsystem_manager| {
                    subsystem_manager.register_fn("test_subsystem", test_subsystem)
                })
                .without_signals()
                .build()
                .unwrap();

            // Request shutdown
            daemon.shutdown();
            assert!(!daemon.is_running());
        })
        .await;

        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[test]
    fn test_daemon_stats() {
        let config = Config::new().unwrap();
        let daemon = Daemon::builder(config).build().unwrap();

        let stats = daemon.get_stats();
        assert_eq!(stats.name, "proc-daemon");
        assert!(stats.uptime.is_none()); // Not started yet
        assert!(!stats.is_shutdown);
    }

    struct TestSubsystemStruct {
        name: String,
    }

    impl TestSubsystemStruct {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    impl Subsystem for TestSubsystemStruct {
        fn run(
            &self,
            shutdown: crate::shutdown::ShutdownHandle,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
            Box::pin(async move {
                #[cfg(feature = "tokio")]
                let mut shutdown = shutdown;
                loop {
                    #[cfg(feature = "tokio")]
                    {
                        tokio::select! {
                            () = shutdown.cancelled() => break,
                            () = tokio::time::sleep(Duration::from_millis(10)) => {}
                        }
                    }

                    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
                    {
                        if shutdown.is_shutdown() {
                            break;
                        }
                        async_std::task::sleep(Duration::from_millis(10)).await;
                    }
                }
                Ok(())
            })
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    #[cfg(feature = "tokio")]
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_daemon_with_struct_subsystem() {
        // Add a test timeout to prevent freezing
        let test_result = tokio::time::timeout(Duration::from_secs(5), async {
            let config = Config::new().unwrap();
            let subsystem = TestSubsystemStruct::new("struct_test");

            let daemon = Daemon::builder(config)
                .with_subsystem(subsystem)
                .without_signals()
                .build()
                .unwrap();

            let stats = daemon.get_stats();
            assert_eq!(stats.subsystem_stats.total_subsystems, 1);

            // Ensure proper cleanup
            daemon.shutdown();
        })
        .await;

        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    #[async_std::test]
    async fn test_daemon_with_struct_subsystem() {
        // Add a test timeout to prevent freezing
        let test_result = async_std::future::timeout(Duration::from_secs(5), async {
            let config = Config::new().unwrap();
            let subsystem = TestSubsystemStruct::new("struct_test");

            let daemon = Daemon::builder(config)
                .with_subsystem(subsystem)
                .without_signals()
                .build()
                .unwrap();

            let stats = daemon.get_stats();
            assert_eq!(stats.subsystem_stats.total_subsystems, 1);

            // Ensure proper cleanup
            daemon.shutdown();
        })
        .await;

        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[cfg(feature = "tokio")]
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_daemon_signal_configuration() {
        // Add a test timeout to prevent freezing
        let test_result = tokio::time::timeout(Duration::from_secs(5), async {
            let config = Config::new().unwrap();
            let signal_config = SignalConfig::new().with_sighup().without_sigint();

            let daemon = Daemon::builder(config)
                .with_signal_config(signal_config)
                .build()
                .unwrap();

            assert!(daemon.signal_handler.is_some());

            // Ensure proper cleanup
            daemon.shutdown();
        })
        .await;

        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    #[async_std::test]
    async fn test_daemon_signal_configuration() {
        // Add a test timeout to prevent freezing
        let test_result = async_std::future::timeout(Duration::from_secs(5), async {
            let config = Config::new().unwrap();
            let signal_config = SignalConfig::new().with_sighup().without_sigint();

            let daemon = Daemon::builder(config)
                .with_signal_config(signal_config)
                .build()
                .unwrap();

            assert!(daemon.signal_handler.is_some());

            // Ensure proper cleanup
            daemon.shutdown();
        })
        .await;

        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[cfg(feature = "tokio")]
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_macro_usage() {
        // Add a test timeout to prevent freezing
        let test_result = tokio::time::timeout(Duration::from_secs(5), async {
            let config = Config::new().unwrap();

            let daemon = Daemon::builder(config)
                .with_task(
                    "macro_test",
                    task!("macro_test", {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }),
                )
                .without_signals()
                .build()
                .unwrap();

            let stats = daemon.get_stats();
            assert_eq!(stats.subsystem_stats.total_subsystems, 1);

            // Ensure proper cleanup
            daemon.shutdown();
        })
        .await;

        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    #[async_std::test]
    async fn test_macro_usage() {
        // Add a test timeout to prevent freezing
        let test_result = async_std::future::timeout(Duration::from_secs(5), async {
            let config = Config::new().unwrap();

            let daemon = Daemon::builder(config)
                .with_task(
                    "macro_test",
                    task!("macro_test", {
                        async_std::task::sleep(Duration::from_millis(1)).await;
                    }),
                )
                .without_signals()
                .build()
                .unwrap();

            let stats = daemon.get_stats();
            assert_eq!(stats.subsystem_stats.total_subsystems, 1);

            // Ensure proper cleanup
            daemon.shutdown();
        })
        .await;

        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }
}
