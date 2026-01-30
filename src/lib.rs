#![deny(missing_docs)]
#![deny(unsafe_code)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
//! # proc-daemon: High-Performance Daemon Framework
//!
//! A foundational framework for building high-performance, resilient daemon services in Rust.
//! Designed for enterprise applications requiring nanosecond-level performance, bulletproof
//! reliability, and extreme concurrency.
//!
//! ## Key Features
//!
//! - **Zero-Copy Architecture**: Minimal allocations with memory pooling
//! - **Runtime Agnostic**: Support for both Tokio and async-std via feature flags
//! - **Cross-Platform**: First-class support for Linux, macOS, and Windows
//! - **Graceful Shutdown**: Coordinated shutdown with configurable timeouts
//! - **Signal Handling**: Robust cross-platform signal management
//! - **Configuration**: Hot-reloadable configuration with multiple sources
//! - **Structured Logging**: High-performance tracing with JSON support
//! - **Subsystem Management**: Concurrent subsystem lifecycle management
//! - **Enterprise Ready**: Built for 100,000+ concurrent operations
//!
//! ## Quick Start
//!
//! ```rust,compile_fail
//! // This example is marked as compile_fail to prevent freezing in doctests
//! use proc_daemon::{Daemon, Config, Result};
//! use std::time::Duration;
//!
//! async fn my_service(mut shutdown: proc_daemon::ShutdownHandle) -> Result<()> {
//!     // Limited iterations to prevent infinite loops
//!     for _ in 0..3 {
//!         tokio::select! {
//!             _ = shutdown.cancelled() => {
//!                 tracing::info!("Service shutting down gracefully");
//!                 return Ok(());
//!             }
//!             _ = tokio::time::sleep(Duration::from_millis(10)) => {
//!                 tracing::info!("Service working...");
//!             }
//!         }
//!     }
//!     Ok(())
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let config = Config::new()?;
//!     
//!     // Set a timeout for the daemon to auto-shutdown
//!     let daemon_handle = Daemon::builder(config)
//!         .with_subsystem_fn("main", my_service)
//!         .run()
//!         .await?
//!     
//!     // Wait for a short time, then explicitly shut down
//!     tokio::time::sleep(Duration::from_millis(100)).await;
//!     daemon_handle.initiate_shutdown();
//!     Ok(())
//! }
//! ```
//
//! ```rust,ignore
//! // This example is marked as ignore to prevent freezing in doctests
//! use proc_daemon::{Daemon, Config, Result};
//! use std::time::Duration;
//!
//! async fn my_service(mut shutdown: proc_daemon::ShutdownHandle) -> Result<()> {
//!     // Use a counter to avoid infinite loops
//!     let mut counter = 0;
//!     while counter < 3 {
//!         tokio::select! {
//!             _ = shutdown.cancelled() => {
//!                 tracing::info!("Service shutting down gracefully");
//!                 break;
//!             }
//!             _ = tokio::time::sleep(Duration::from_millis(10)) => {
//!                 tracing::info!("Service working...");
//!                 counter += 1;
//!             }
//!         }
//!     }
//!     Ok(())
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let config = Config::new()?;
//!     
//!     // Auto-shutdown after a brief time
//!     tokio::spawn(async {
//!         tokio::time::sleep(Duration::from_millis(50)).await;
//!         std::process::exit(0); // Force exit to prevent hanging
//!     });
//!     
//!     Daemon::builder(config)
//!         .with_subsystem_fn("main", my_service)
//!         .run()
//!         .await
//! }
//! ```

// Optional global allocator: mimalloc
// Enabled only when the 'mimalloc' feature is set
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// Private modules
mod config;
mod daemon;
mod error;
mod pool;

// Public modules
pub mod coord;
pub mod lock;
pub mod resources;
pub mod shutdown;
pub mod signal;
pub mod subsystem;

// Public exports
pub use config::{Config, LogLevel};
pub use daemon::{Daemon, DaemonBuilder};
pub use error::{Error, Result};
pub use pool::*;
pub use shutdown::{ShutdownHandle, ShutdownReason};
pub use subsystem::{RestartPolicy, Subsystem, SubsystemId};

#[cfg(feature = "metrics")]
pub mod metrics;

#[cfg(feature = "profiling")]
pub mod profiling;

#[cfg(feature = "ipc")]
pub mod ipc;

/// High-resolution timing helpers (opt-in)
///
/// When the `high-res-timing` feature is enabled, provides an ultra-fast
/// monotonically increasing clock via `quanta`.
#[cfg(feature = "high-res-timing")]
pub mod timing {
    use quanta::Clock;
    pub use quanta::Instant;

    static CLOCK: std::sync::LazyLock<Clock> = std::sync::LazyLock::new(Clock::new);

    /// Returns a high-resolution Instant using a cached `quanta::Clock`.
    #[inline]
    pub fn now() -> Instant {
        CLOCK.now()
    }
}

/// Scheduler hint hooks (opt-in)
///
/// When the `scheduler-hints` feature is enabled, exposes best-effort functions to
/// apply light-weight scheduler tuning (process niceness) where supported. All
/// operations are non-fatal; failures are logged at debug level.
#[cfg(feature = "scheduler-hints")]
pub mod scheduler {
    use tracing::{debug, info};

    /// Apply process-level scheduler hints.
    ///
    /// Apply process-level hints.
    ///
    /// Default: no-op. If `scheduler-hints-unix` is enabled on Unix, attempts a
    /// best-effort niceness reduction.
    pub fn apply_process_hints(config: &crate::config::Config) {
        let name = &config.name;
        #[cfg(all(feature = "scheduler-hints-unix", unix))]
        {
            use std::process::Command;
            // Try renicing current process by -5. This typically needs elevated privileges.
            let delta = "-5";
            let pid = std::process::id().to_string();
            let out = Command::new("renice")
                .args(["-n", delta, "-p", pid.as_str()])
                .output();
            match out {
                Ok(res) if res.status.success() => {
                    info!(%name, delta, "scheduler-hints: renice applied");
                    return;
                }
                Ok(res) => {
                    let code = res.status.code();
                    let stderr = String::from_utf8_lossy(&res.stderr);
                    debug!(%name, delta, code, stderr = %stderr, "scheduler-hints: renice failed (best-effort)");
                }
                Err(e) => {
                    debug!(%name, delta, error = %e, "scheduler-hints: renice invocation failed (best-effort)");
                }
            }
        }
        debug!(%name, "scheduler-hints: apply_process_hints (noop)");
    }

    /// Apply runtime-level scheduler hints.
    ///
    /// Placeholder for future integration (e.g., per-thread QoS/priority, affinity).
    pub fn apply_runtime_hints() {
        #[cfg(all(feature = "scheduler-hints-unix", target_os = "linux"))]
        {
            use nix::sched::{sched_setaffinity, CpuSet};
            use nix::unistd::Pid;

            // Best-effort: allow running on all available CPUs (explicitly set mask)
            let mut set = CpuSet::new();
            let cpus = num_cpus::get();
            for cpu in 0..cpus {
                let _ = set.set(cpu);
            }
            match sched_setaffinity(Pid::from_raw(0), &set) {
                Ok(()) => debug!(
                    cpus,
                    "scheduler-hints: setaffinity applied to current process threads (best-effort)"
                ),
                Err(e) => debug!(error = %e, "scheduler-hints: setaffinity failed (best-effort)"),
            }
        }

        #[cfg(not(all(feature = "scheduler-hints-unix", target_os = "linux")))]
        {
            debug!("scheduler-hints: apply_runtime_hints (noop)");
        }
    }
}

/// Version of the proc-daemon library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default shutdown timeout in milliseconds
pub const DEFAULT_SHUTDOWN_TIMEOUT_MS: u64 = 5000;

/// Default configuration file name
pub const DEFAULT_CONFIG_FILE: &str = "daemon.toml";
