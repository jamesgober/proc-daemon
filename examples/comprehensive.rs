//! Comprehensive example demonstrating all proc-daemon features.
//!
//! This example shows how to build a production-ready daemon service with
//! multiple subsystems, configuration management, signal handling, and metrics.

use proc_daemon::{Config, Daemon, LogLevel, RestartPolicy, ShutdownHandle, Subsystem};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tracing::{info, warn};

// Define runtime-specific imports
#[cfg(feature = "tokio")]
use tokio::time::sleep as async_sleep;

#[cfg(all(feature = "async-std", not(feature = "tokio")))]
use async_std::task::sleep as async_sleep;

// Fallback when no async runtime is enabled: provide a no-op async sleep
#[cfg(not(any(feature = "tokio", feature = "async-std")))]
async fn async_sleep(_d: Duration) {}

/// Example HTTP server subsystem
struct HttpServer {
    port: u16,
}

// Fallback main when no runtime is enabled so this example still compiles
#[cfg(not(any(feature = "tokio", feature = "async-std")))]
fn main() {
    eprintln!(
        "This example requires a runtime feature. Enable either 'tokio' or 'async-std' features."
    );
}

impl HttpServer {
    fn new(port: u16) -> Self {
        Self { port }
    }
}

impl Subsystem for HttpServer {
    fn run(
        &self,
        shutdown: ShutdownHandle,
    ) -> Pin<Box<dyn Future<Output = proc_daemon::Result<()>> + Send>> {
        let port = self.port;
        Box::pin(async move {
            info!("Starting HTTP server on port {}", port);

            // Simulate server initialization
            async_sleep(Duration::from_millis(100)).await;
            info!("HTTP server listening on port {}", port);

            let mut request_count = 0u64;

            #[cfg(feature = "tokio")]
            {
                let mut shutdown = shutdown;
                loop {
                    tokio::select! {
                        _ = shutdown.cancelled() => {
                            info!("HTTP server shutting down gracefully");
                            // Simulate graceful shutdown
                            async_sleep(Duration::from_millis(500)).await;
                            info!("HTTP server stopped after handling {} requests", request_count);
                            break;
                        }
                        _ = async_sleep(Duration::from_millis(100)) => {
                            // Simulate handling requests
                            request_count += fastrand::u64(1..10);
                            if request_count % 100 == 0 {
                                info!("HTTP server handled {} requests", request_count);
                            }
                        }
                    }
                }
            }

            #[cfg(all(feature = "async-std", not(feature = "tokio")))]
            {
                loop {
                    // Check if shutdown is requested
                    if shutdown.is_shutdown() {
                        info!("HTTP server shutting down gracefully");
                        // Simulate graceful shutdown
                        async_sleep(Duration::from_millis(500)).await;
                        info!(
                            "HTTP server stopped after handling {} requests",
                            request_count
                        );
                        break;
                    }

                    // Sleep for a bit
                    async_sleep(Duration::from_millis(100)).await;

                    // Simulate handling requests
                    request_count += fastrand::u64(1..10);
                    if request_count % 100 == 0 {
                        info!("HTTP server handled {} requests", request_count);
                    }
                }
            }

            Ok(())
        })
    }

    fn name(&self) -> &str {
        "http_server"
    }

    fn restart_policy(&self) -> RestartPolicy {
        RestartPolicy::ExponentialBackoff {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            max_attempts: 5,
        }
    }

    fn health_check(&self) -> Option<Box<dyn Fn() -> bool + Send + Sync>> {
        Some(Box::new(|| {
            // Simulate health check (e.g., check if port is listening)
            true
        }))
    }
}

/// Example database connection pool subsystem
struct DatabasePool {
    max_connections: u32,
}

impl DatabasePool {
    fn new(max_connections: u32) -> Self {
        Self { max_connections }
    }
}

impl Subsystem for DatabasePool {
    fn run(
        &self,
        shutdown: ShutdownHandle,
    ) -> Pin<Box<dyn Future<Output = proc_daemon::Result<()>> + Send>> {
        let max_connections = self.max_connections;
        Box::pin(async move {
            info!(
                "Initializing database connection pool (max: {})",
                max_connections
            );

            // Simulate pool initialization
            async_sleep(Duration::from_millis(200)).await;

            let mut active_connections = 0u32;

            #[cfg(feature = "tokio")]
            {
                let mut shutdown = shutdown;
                loop {
                    tokio::select! {
                        _ = shutdown.cancelled() => {
                            info!("Database pool shutting down, closing {} connections", active_connections);
                            // Simulate connection cleanup
                            while active_connections > 0 {
                                active_connections -= 1;
                                async_sleep(Duration::from_millis(10)).await;
                            }
                            info!("Database pool shutdown complete");
                            break;
                        }
                        _ = async_sleep(Duration::from_millis(500)) => {
                            // Simulate connection management
                            let new_connections = fastrand::u32(0..5);
                            active_connections = (active_connections + new_connections).min(max_connections);

                            if active_connections > max_connections * 8 / 10 {
                                warn!("Database pool at {}% capacity",
                                      active_connections * 100 / max_connections);
                            }
                        }
                    }
                }
            }

            #[cfg(all(feature = "async-std", not(feature = "tokio")))]
            {
                loop {
                    // Check if shutdown is requested
                    if shutdown.is_shutdown() {
                        info!(
                            "Database pool shutting down, closing {} connections",
                            active_connections
                        );
                        // Simulate connection cleanup
                        while active_connections > 0 {
                            active_connections -= 1;
                            async_sleep(Duration::from_millis(10)).await;
                        }
                        info!("Database pool shutdown complete");
                        break;
                    }

                    // Sleep for a bit
                    async_sleep(Duration::from_millis(500)).await;

                    // Simulate connection management
                    let new_connections = fastrand::u32(0..5);
                    active_connections =
                        (active_connections + new_connections).min(max_connections);

                    if active_connections > max_connections * 8 / 10 {
                        warn!(
                            "Database pool at {}% capacity",
                            active_connections * 100 / max_connections
                        );
                    }
                }
            }

            Ok(())
        })
    }

    fn name(&self) -> &str {
        "database_pool"
    }

    fn restart_policy(&self) -> RestartPolicy {
        RestartPolicy::OnFailure
    }
}

/// Example background task processor
async fn background_processor(shutdown: ShutdownHandle) -> proc_daemon::Result<()> {
    info!("Starting background task processor");

    let mut tasks_processed = 0u64;

    #[cfg(feature = "tokio")]
    {
        let mut shutdown = shutdown;
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    info!("Background processor shutting down after processing {} tasks", tasks_processed);
                    break;
                }
                _ = async_sleep(Duration::from_millis(50)) => {
                    // Simulate processing background tasks
                    tasks_processed += fastrand::u64(1..5);

                    if tasks_processed % 50 == 0 {
                        info!("Background processor completed {} tasks", tasks_processed);
                    }

                    // Simulate occasional processing error
                    if fastrand::f32() < 0.001 {
                        warn!("Background processor encountered non-critical error");
                    }
                }
            }
        }
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    {
        loop {
            if shutdown.is_shutdown() {
                info!(
                    "Background processor shutting down after processing {} tasks",
                    tasks_processed
                );
                break;
            }

            async_sleep(Duration::from_millis(50)).await;

            // Simulate processing background tasks
            tasks_processed += fastrand::u64(1..5);

            if tasks_processed % 50 == 0 {
                info!("Background processor completed {} tasks", tasks_processed);
            }

            // Simulate occasional processing error
            if fastrand::f32() < 0.001 {
                warn!("Background processor encountered non-critical error");
            }
        }
    }

    Ok(())
}

/// Example metrics reporter
async fn metrics_reporter(shutdown: ShutdownHandle) -> proc_daemon::Result<()> {
    info!("Starting metrics reporter");

    #[cfg(feature = "tokio")]
    {
        let mut shutdown = shutdown;
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    info!("Metrics reporter shutting down");
                    break;
                }
                _ = async_sleep(Duration::from_secs(10)) => {
                    // Simulate metrics reporting
                    let cpu_usage = fastrand::f32() * 100.0;
                    let memory_usage = fastrand::f32() * 100.0;
                    let disk_usage = fastrand::f32() * 100.0;

                    info!(
                        "System metrics - CPU: {:.1}%, Memory: {:.1}%, Disk: {:.1}%",
                        cpu_usage, memory_usage, disk_usage
                    );

                    if cpu_usage > 90.0 || memory_usage > 90.0 {
                        warn!("High resource usage detected!");
                    }
                }
            }
        }
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    {
        loop {
            if shutdown.is_shutdown() {
                info!("Metrics reporter shutting down");
                break;
            }

            async_sleep(Duration::from_secs(10)).await;

            // Simulate metrics reporting
            let cpu_usage = fastrand::f32() * 100.0;
            let memory_usage = fastrand::f32() * 100.0;
            let disk_usage = fastrand::f32() * 100.0;

            info!(
                "System metrics - CPU: {:.1}%, Memory: {:.1}%, Disk: {:.1}%",
                cpu_usage, memory_usage, disk_usage
            );

            if cpu_usage > 90.0 || memory_usage > 90.0 {
                warn!("High resource usage detected!");
            }
        }
    }

    Ok(())
}

#[cfg(feature = "tokio")]
#[tokio::main]
async fn main() -> proc_daemon::Result<()> {
    run_daemon().await
}

#[cfg(all(feature = "async-std", not(feature = "tokio")))]
#[async_std::main]
async fn main() -> proc_daemon::Result<()> {
    run_daemon().await
}

async fn run_daemon() -> proc_daemon::Result<()> {
    // Create configuration
    let config = Config::builder()
        .name("example-daemon")
        .log_level(LogLevel::Info)
        .json_logging(false) // Use structured text logs for this example
        .shutdown_timeout(Duration::from_secs(30))
        .unwrap()
        .worker_threads(4)
        .enable_metrics(true)
        .build()?;

    // Build and run the daemon
    Daemon::builder(config)
        // Add HTTP server subsystem
        .with_subsystem(HttpServer::new(8080))
        // Add database pool subsystem
        .with_subsystem(DatabasePool::new(20))
        // Add background task processor
        .with_subsystem_fn("background_processor", |subsystem_manager| {
            subsystem_manager.register_fn("background_processor", background_processor)
        })
        // Add metrics reporter
        .with_task("metrics_reporter", metrics_reporter)
        // Configure signal handling
        .with_signals(true, true) // Handle SIGTERM and SIGINT
        // Run the daemon
        .run()
        .await
}
