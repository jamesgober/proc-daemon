//! Core daemon implementation with builder pattern.
//!
//! This module provides the main `Daemon` struct and `DaemonBuilder` for creating
//! high-performance, resilient daemon services. The builder pattern allows for
//! flexible configuration while maintaining zero-copy performance characteristics.

use std::future::Future;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn, instrument};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::shutdown::{ShutdownCoordinator, ShutdownReason};
use crate::signal::{SignalHandler, SignalConfig};
use crate::subsystem::{Subsystem, SubsystemManager, SubsystemId};

/// Type alias for subsystem registration function
type SubsystemRegistrationFn = Box<dyn FnOnce(&SubsystemManager) -> SubsystemId + Send + 'static>;

/// Main daemon instance that coordinates all subsystems and handles lifecycle.
pub struct Daemon {
    /// Configuration
    config: Arc<Config>,
    /// Shutdown coordination
    shutdown_coordinator: ShutdownCoordinator,
    /// Subsystem management
    subsystem_manager: SubsystemManager,
    /// Signal handling
    signal_handler: Option<Arc<SignalHandler>>,
    /// Start time
    started_at: Option<Instant>,
}

impl Daemon {
    /// Create a new daemon builder with the provided configuration.
    #[must_use] pub fn builder(config: Config) -> DaemonBuilder {
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

        // Initialize logging
        self.init_logging()?;

        // Validate configuration
        self.config.validate()?;

        // Start all subsystems
        if let Err(e) = self.subsystem_manager.start_all().await {
            error!(error = %e, "Failed to start all subsystems");
            return Err(e);
        }

        // Start signal handling in the background
        let signal_task = self.signal_handler.as_ref().map_or_else(
            || None,
            |signal_handler| {
                let handler = Arc::clone(signal_handler);
                Some(Self::spawn_signal_handler(handler))
            }
        );

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
                let unhealthy: Vec<_> = health_results
                    .iter()
                    .filter(|(_, _, healthy)| !healthy)
                    .map(|(id, name, _)| (id, name))
                    .collect();

                if !unhealthy.is_empty() {
                    warn!("Unhealthy subsystems detected: {:?}", unhealthy);
                    // Could implement auto-restart logic here
                }
            }

            // Sleep for a short interval
            #[cfg(feature = "tokio")]
            tokio::time::sleep(self.config.health_check_interval()).await;

            #[cfg(all(feature = "async-std", not(feature = "tokio")))]
            async_std::task::sleep(self.config.health_check_interval()).await;
        }

        // Graceful shutdown sequence
        info!("Shutdown initiated, beginning graceful shutdown");
        
        // Stop signal handling
        if let Some(signal_handler) = &self.signal_handler {
            signal_handler.stop();
        }

        // Wait for signal handler task to complete
        if let Some(task) = signal_task {
            #[cfg(feature = "tokio")]
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
                error!(error = %e, "Force shutdown timeout exceeded, exiting immediately");
            }
        }

        let elapsed = self.started_at.map(|t| t.elapsed());
        info!(uptime = ?elapsed, "Daemon shutdown complete");

        Ok(())
    }

    /// Initialize the logging system based on configuration.
    fn init_logging(&self) -> Result<()> {
        use tracing_subscriber::{EnvFilter, FmtSubscriber};
        use tracing_subscriber::fmt::format::FmtSpan;

        let level: tracing::Level = self.config.logging.level.into();
        let filter = EnvFilter::from_default_env()
            .add_directive(level.into());

        let base_subscriber = FmtSubscriber::builder()
            .with_env_filter(filter)
            .with_span_events(FmtSpan::CLOSE)
            .with_target(true)
            .with_thread_ids(true)
            .with_thread_names(true);

        // Configure output format
        if self.config.is_json_logging() {
            #[cfg(feature = "json-logs")]
            {
                let json_subscriber = base_subscriber
                    .json()
                    .flatten_event(true)
                    .with_current_span(false);
                
                tracing::subscriber::set_global_default(json_subscriber.finish())
                    .map_err(|e| Error::config(format!("Failed to initialize JSON logging: {}", e)))?;
            }
            
            #[cfg(not(feature = "json-logs"))]
            {
                return Err(Error::config("JSON logging requested but feature not enabled"));
            }
        }
        
        // Regular non-JSON logging
        let regular_subscriber = base_subscriber
            .with_ansi(self.config.is_colored_logging())
            .compact();
            
        tracing::subscriber::set_global_default(regular_subscriber.finish())
            .map_err(|e| Error::config(format!("Failed to initialize logging: {e}")))?;

        debug!("Logging initialized with level: {:?}", self.config.logging.level);
        Ok(())
    }

    /// Spawn the signal handler task.
    #[cfg(feature = "tokio")]
    fn spawn_signal_handler(handler: Arc<SignalHandler>) -> tokio::task::JoinHandle<Result<()>> {
        tokio::spawn(async move {
            handler.handle_signals().await
        })
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    fn spawn_signal_handler(handler: Arc<SignalHandler>) -> async_std::task::JoinHandle<Result<()>> {
        async_std::task::spawn(async move {
            handler.handle_signals().await
        })
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
        self.shutdown_coordinator.initiate_shutdown(ShutdownReason::Requested)
    }

    /// Check if the daemon is running.
    pub fn is_running(&self) -> bool {
        !self.shutdown_coordinator.is_shutdown()
    }

    /// Get the daemon configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }
}

impl Clone for Daemon {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            shutdown_coordinator: self.shutdown_coordinator.clone(),
            subsystem_manager: self.subsystem_manager.clone(),
            signal_handler: self.signal_handler.clone(),
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
}

impl DaemonBuilder {
    /// Create a new daemon builder with the provided configuration.
    #[must_use] pub fn new(config: Config) -> Self {
        Self {
            config,
            // Pre-allocate the subsystems vector with a reasonable capacity
            subsystems: Vec::with_capacity(16),
            signal_config: None,
            enable_signals: true,
        }
    }

    /// Configure signal handling.
    #[must_use]
    pub fn with_signal_config(mut self, config: SignalConfig) -> Self {
        self.signal_config = Some(config);
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
    pub fn with_task<F, Fut>(
        mut self,
        name: &str,
        task_fn: F
    ) -> Self 
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
    pub fn with_subsystem<S>(
        mut self,
        subsystem: S
    ) -> Self 
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
    pub fn with_subsystem_fn<F>(
        mut self,
        name: &str,
        register_fn: F
    ) -> Self
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
            Some(Arc::new(SignalHandler::new(shutdown_coordinator.clone())))
        } else {
            None
        };

        Ok(Daemon {
            config: Arc::new(self.config),
            shutdown_coordinator,
            subsystem_manager,
            signal_handler,
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
    ($name:expr, $body:block) => {
        |mut shutdown: $crate::shutdown::ShutdownHandle| async move {
            loop {
                #[cfg(feature = "tokio")]
                {
                    tokio::select! {
                        _ = shutdown.cancelled() => {
                            tracing::info!("Task '{}' shutting down", $name);
                            break;
                        }
                        _ = async $body => {}
                    }
                }

                #[cfg(all(feature = "async-std", not(feature = "tokio")))]
                {
                    if shutdown.is_shutdown() {
                        tracing::info!("Task '{}' shutting down", $name);
                        break;
                    }
                    $body.await;
                }
            }
            Ok(())
        }
    };
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use super::*;
    use std::time::Duration;

    async fn test_subsystem(mut shutdown: crate::shutdown::ShutdownHandle) -> Result<()> {
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
        }).await;
        
        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

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
        }).await;
        
        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[tokio::test]
    async fn test_daemon_shutdown() {
        // Add a test timeout to prevent freezing
        let test_result = tokio::time::timeout(Duration::from_secs(5), async {
            let config = Config::builder()
                .name("test-daemon")
                .shutdown_timeout(Duration::from_millis(100))
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
        }).await;
        
        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[test]
    fn test_daemon_stats() {
        let config = Config::new().unwrap();
        let daemon = Daemon::builder(config)
            .build()
            .unwrap();

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
        fn run(&self, mut shutdown: crate::shutdown::ShutdownHandle) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
            Box::pin(async move {
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
        }).await;
        
        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[tokio::test]
    async fn test_daemon_signal_configuration() {
        // Add a test timeout to prevent freezing
        let test_result = tokio::time::timeout(Duration::from_secs(5), async {
            let config = Config::new().unwrap();
            let signal_config = SignalConfig::new()
                .with_sighup()
                .without_sigint();

            let daemon = Daemon::builder(config)
                .with_signal_config(signal_config)
                .build()
                .unwrap();

            assert!(daemon.signal_handler.is_some());
            
            // Ensure proper cleanup
            daemon.shutdown();
        }).await;
        
        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[tokio::test]
    async fn test_macro_usage() {
        // Add a test timeout to prevent freezing
        let test_result = tokio::time::timeout(Duration::from_secs(5), async {
            let config = Config::new().unwrap();
            
            let daemon = Daemon::builder(config)
                .with_task("macro_test", task!("macro_test", {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }))
                .without_signals()
                .build()
                .unwrap();

            let stats = daemon.get_stats();
            assert_eq!(stats.subsystem_stats.total_subsystems, 1);
            
            // Ensure proper cleanup
            daemon.shutdown();
        }).await;
        
        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }
}
