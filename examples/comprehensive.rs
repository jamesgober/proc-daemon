//! Comprehensive example demonstrating all proc-daemon features.
//!
//! This example shows how to build a production-ready daemon service with
//! multiple subsystems, configuration management, signal handling, and metrics.

use proc_daemon::{Daemon, Config, LogLevel, RestartPolicy, Subsystem, ShutdownHandle};
use std::pin::Pin;
use std::future::Future;
use std::time::Duration;
use tracing::{info, warn};

/// Example HTTP server subsystem
struct HttpServer {
    port: u16,
}

impl HttpServer {
    fn new(port: u16) -> Self {
        Self { port }
    }
}

impl Subsystem for HttpServer {
    fn run(&self, mut shutdown: ShutdownHandle) -> Pin<Box<dyn Future<Output = proc_daemon::Result<()>> + Send>> {
        let port = self.port;
        Box::pin(async move {
            info!("Starting HTTP server on port {}", port);
            
            // Simulate server initialization
            tokio::time::sleep(Duration::from_millis(100)).await;
            info!("HTTP server listening on port {}", port);

            let mut request_count = 0u64;
            
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => {
                        info!("HTTP server shutting down gracefully");
                        // Simulate graceful shutdown
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        info!("HTTP server stopped after handling {} requests", request_count);
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        // Simulate handling requests
                        request_count += fastrand::u64(1..10);
                        if request_count % 100 == 0 {
                            info!("HTTP server handled {} requests", request_count);
                        }
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
    fn run(&self, mut shutdown: ShutdownHandle) -> Pin<Box<dyn Future<Output = proc_daemon::Result<()>> + Send>> {
        let max_connections = self.max_connections;
        Box::pin(async move {
            info!("Initializing database connection pool (max: {})", max_connections);
            
            // Simulate pool initialization
            tokio::time::sleep(Duration::from_millis(200)).await;
            
            let mut active_connections = 0u32;
            
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => {
                        info!("Database pool shutting down, closing {} connections", active_connections);
                        // Simulate connection cleanup
                        while active_connections > 0 {
                            active_connections -= 1;
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                        info!("Database pool shutdown complete");
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {
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
async fn background_processor(mut shutdown: ShutdownHandle) -> proc_daemon::Result<()> {
    info!("Starting background task processor");
    
    let mut tasks_processed = 0u64;
    
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                info!("Background processor shutting down after processing {} tasks", tasks_processed);
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
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
    
    Ok(())
}

/// Example metrics reporter
async fn metrics_reporter(mut shutdown: ShutdownHandle) -> proc_daemon::Result<()> {
    info!("Starting metrics reporter");
    
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                info!("Metrics reporter shutting down");
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(10)) => {
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
    
    Ok(())
}

#[tokio::main]
async fn main() -> proc_daemon::Result<()> {
    // Create configuration
    let config = Config::builder()
        .name("example-daemon")
        .log_level(LogLevel::Info)
        .json_logging(false)  // Use structured text logs for this example
        .shutdown_timeout(Duration::from_secs(30))
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
        .with_signals(true, true)  // Handle SIGTERM and SIGINT
        
        // Run the daemon
        .run()
        .await
}
