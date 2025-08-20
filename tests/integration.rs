//! Integration tests for proc-daemon.

use proc_daemon::{Daemon, Config, LogLevel};
use std::time::Duration;
use tokio::time::timeout;

// Function removed to eliminate dead code warning

#[tokio::test]
async fn test_basic_daemon_creation() {
    let test_timeout = Duration::from_secs(2);
    let config = Config::builder()
        .name("test-daemon")
        .log_level(LogLevel::Error) // Reduce log noise in tests
        .build()
        .unwrap();

    let daemon = Daemon::builder(config)
        .with_task("test_worker", |mut shutdown| async move {
            shutdown.cancelled().await;
            Ok(())
        })
        .without_signals()
        .build()
        .unwrap();

    // Verify basic properties
    assert!(daemon.is_running());
    assert_eq!(daemon.config().name, "test-daemon");
    
    // Test shutdown with timeout
    let shutdown_result = timeout(test_timeout, async {
        daemon.shutdown();
        assert!(!daemon.is_running());
    }).await;
    
    assert!(shutdown_result.is_ok(), "Test timed out during shutdown");
}

#[tokio::test]
async fn test_config_builder() {
    let config = Config::builder()
        .name("builder-test")
        .log_level(LogLevel::Debug)
        .json_logging(true)
        .shutdown_timeout(Duration::from_secs(60))
        .force_shutdown_timeout(Duration::from_secs(120))
        .kill_timeout(Duration::from_secs(180))
        .worker_threads(8)
        .build()
        .unwrap();

    assert_eq!(config.name, "builder-test");
    assert_eq!(config.logging.level, LogLevel::Debug);
    assert!(config.logging.json);
    assert_eq!(config.shutdown.timeout_ms, 60000);
    assert_eq!(config.performance.worker_threads, 8);
}

#[tokio::test]
async fn test_daemon_stats() {
    let test_timeout = Duration::from_secs(2);
    let config = Config::new().unwrap();
    let daemon = Daemon::builder(config)
        .with_task("stats_worker", |mut shutdown| async move {
            shutdown.cancelled().await;
            Ok(())
        })
        .without_signals()
        .build()
        .unwrap();

    let stats_result = timeout(test_timeout, async {
        let stats = daemon.get_stats();
        assert_eq!(stats.name, "proc-daemon");
        assert!(stats.uptime.is_none()); // Not started yet
        assert!(!stats.is_shutdown);
        assert_eq!(stats.subsystem_stats.total_subsystems, 1);
    }).await;
    
    assert!(stats_result.is_ok(), "Test timed out during stats check");
}

#[tokio::test]
async fn test_multiple_subsystems() {
    let test_timeout = Duration::from_secs(2);
    let config = Config::builder()
        .name("multi-test-daemon")
        .log_level(LogLevel::Error)
        .build()
        .unwrap();

    let daemon = Daemon::builder(config)
        .with_task("worker1", |mut shutdown| async move {
            shutdown.cancelled().await;
            Ok(())
        })
        .with_task("worker2", |mut shutdown| async move {
            shutdown.cancelled().await;
            Ok(())
        })
        .without_signals()
        .build()
        .unwrap();

    let stats = daemon.get_stats();
    assert_eq!(stats.subsystem_stats.total_subsystems, 2);
    
    // Shutdown with timeout
    let shutdown_result = timeout(test_timeout, async {
        daemon.shutdown();
        assert!(!daemon.is_running());
    }).await;
    
    assert!(shutdown_result.is_ok(), "Test timed out during shutdown");
}

#[tokio::test]
async fn test_daemon_with_defaults() {
    let test_timeout = Duration::from_secs(2);
    let builder = Daemon::with_defaults().unwrap();
    let daemon = builder
        .with_task("simple_task", |mut shutdown| async move {
            shutdown.cancelled().await;
            Ok(())
        })
        .without_signals()
        .build()
        .unwrap();

    assert!(daemon.is_running());
    
    // Shutdown with timeout
    let shutdown_result = timeout(test_timeout, async {
        daemon.shutdown();
        assert!(!daemon.is_running());
    }).await;
    
    assert!(shutdown_result.is_ok(), "Test timed out during shutdown");
}
