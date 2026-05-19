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
//! The simplest shape — `Daemon::new()` (v1.1.0+) is the infallible
//! shortcut over `Config::default()`. Use `Daemon::builder(config)` when
//! you need explicit configuration.
//!
//! ```no_run
//! use proc_daemon::{Daemon, ShutdownHandle};
//! use std::time::Duration;
//!
//! async fn my_service(mut shutdown: ShutdownHandle) -> proc_daemon::Result<()> {
//!     loop {
//!         tokio::select! {
//!             () = shutdown.cancelled() => {
//!                 tracing::info!("shutting down gracefully");
//!                 break;
//!             }
//!             () = tokio::time::sleep(Duration::from_secs(1)) => {
//!                 tracing::info!("working…");
//!             }
//!         }
//!     }
//!     Ok(())
//! }
//!
//! #[tokio::main]
//! async fn main() -> proc_daemon::Result<()> {
//!     Daemon::new()
//!         .with_task("my_service", my_service)
//!         .run()
//!         .await
//! }
//! ```
//!
//! With explicit configuration:
//!
//! ```no_run
//! use proc_daemon::{Config, Daemon, LogLevel};
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> proc_daemon::Result<()> {
//!     let config = Config::builder()
//!         .name("my-service")
//!         .log_level(LogLevel::Info)
//!         .shutdown_timeout(Duration::from_secs(30))?
//!         .force_shutdown_timeout(Duration::from_secs(45))?
//!         .kill_timeout(Duration::from_secs(60))?
//!         .build()?;
//!
//!     Daemon::builder(config)
//!         .with_task("worker", |mut shutdown| async move {
//!             shutdown.cancelled().await;
//!             Ok(())
//!         })
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

#[cfg(any(feature = "profiling", feature = "heap-profiling"))]
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
    use tracing::debug;

    #[cfg(all(feature = "scheduler-hints-unix", unix))]
    use tracing::info;

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
