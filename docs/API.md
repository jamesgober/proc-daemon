<div align="center">
    <img width="108xpx" src="../media/proc-rs-orange.svg" alt="High-Performance Process EcoSystem for Rust">
    <h1>
        <strong>Process Daemon</strong>
        <sup><br><sub>API REFERENCE — v1.1.1</sub><br></sup>
    </h1>
</div>

**Status:** ✅ Stable — production-ready framework for high-performance daemon services.

This is the canonical API reference. Versioned release notes live in
[`docs/release-notes/`](./release-notes/). Performance numbers live in
[`PERFORMANCE.md`](./PERFORMANCE.md). Contribution norms live in
[`PRINCIPLES.md`](./PRINCIPLES.md).

## Table of Contents

1. [Installation](#installation)
2. [Core Concepts](#core-concepts)
3. [Constructing a Daemon](#constructing-a-daemon)
4. [`DaemonBuilder`](#daemonbuilder)
5. [The `Subsystem` Trait](#the-subsystem-trait)
6. [`RestartPolicy`](#restartpolicy)
7. [`SubsystemState` & `SubsystemEvent`](#subsystemstate--subsystemevent)
8. [`ShutdownHandle` & `ShutdownCoordinator`](#shutdownhandle--shutdowncoordinator)
9. [`Config` & `ConfigBuilder`](#config--configbuilder)
10. [Error Handling](#error-handling)
11. [Optional modules](#optional-modules)
    - [`metrics`](#metrics-feature)
    - [`resources`](#resources)
    - [`signal`](#signal-handling)
    - [`lock`](#instance-locking)
    - [`pool`](#object-pools)
    - [`ipc`](#ipc-feature)
    - [`profiling`](#profiling)
    - [`coord`](#coordination-primitives)
    - [`timing`](#high-resolution-timing)
    - [`scheduler`](#scheduler-hints)
12. [Feature Flags](#feature-flags)
13. [Platform Support](#platform-support)
14. [Best Practices](#best-practices)

---

## Installation

```toml
[dependencies]
proc-daemon = "1.1.1"
```

With specific optional features:

```toml
[dependencies]
proc-daemon = { version = "1.1.1", features = ["tokio", "metrics", "high-res-timing"] }
```

Convenience meta-feature (`tokio`, `metrics`, `console`, `json-logs`,
`config-watch`, `ipc`):

```toml
[dependencies]
proc-daemon = { version = "1.1.1", features = ["full"] }
```

**MSRV:** Rust 1.82.0.

---

## Core Concepts

### Daemon

The `Daemon` is the running process. It owns:

- Signal handling (cross-platform: SIGTERM/SIGINT/SIGQUIT/SIGHUP on Unix; Ctrl-C and console events on Windows).
- Subsystem lifecycle (start, monitor, restart, stop).
- Shutdown coordination (graceful → force → kill ladder with independent timeouts).
- Optional metrics, hot-reload, IPC, and resource tracking.

Construct via `Daemon::new()` (infallible — uses `Config::default()`) or
`Daemon::builder(config)` for explicit configuration.

### Subsystem

Any long-running unit of work the daemon manages. Can be:

- A function: `with_task("name", fn)` — simplest case, takes a `ShutdownHandle`, returns `Result<()>`.
- A trait impl: `with_subsystem(impl Subsystem)` — for stateful subsystems that need health checks or custom restart policies.
- A custom registration: `with_subsystem_fn("name", |mgr| mgr.register_fn(...))` — for advanced lifetime patterns.

### ShutdownHandle

Every subsystem receives a `ShutdownHandle`. The handle exposes:

- `cancelled().await` — resolves when shutdown is initiated (use in `tokio::select!`).
- `is_shutdown()` — synchronous check.
- `ready()` — mark this subsystem as cleanly shut down (the coordinator uses this to know when graceful shutdown completed).

### Coordinated shutdown

When a signal arrives (or `daemon.shutdown()` is called), the
`ShutdownCoordinator` broadcasts to all handles, then waits up to:

1. `graceful_timeout_ms` for subsystems to mark themselves `ready`.
2. `force_timeout_ms` for stragglers to be force-aborted.
3. `kill_timeout_ms` as the final ceiling.

---

## Constructing a Daemon

### `Daemon::new() -> DaemonBuilder` *(v1.1.0+)*

Infallible. Uses `Config::default()`. Preferred entry point for the
common case.

```rust
use proc_daemon::Daemon;

# async fn example() -> proc_daemon::Result<()> {
Daemon::new()
    .with_task("worker", |mut shutdown| async move {
        shutdown.cancelled().await;
        Ok(())
    })
    .run()
    .await
# }
```

### `Daemon::builder(config: Config) -> DaemonBuilder`

Use when you need explicit configuration:

```rust
use proc_daemon::{Config, Daemon, LogLevel};
use std::time::Duration;

# fn example() -> proc_daemon::Result<()> {
let config = Config::builder()
    .name("my-service")
    .log_level(LogLevel::Info)
    .shutdown_timeout(Duration::from_secs(30))?
    .force_shutdown_timeout(Duration::from_secs(45))?
    .kill_timeout(Duration::from_secs(60))?
    .worker_threads(4)
    .build()?;

let _builder = Daemon::builder(config);
# Ok(())
# }
```

### `Daemon::with_defaults() -> Result<DaemonBuilder>`

Retained for backward compatibility. Prefer `Daemon::new()`. The
default `Config` is valid by construction, so the `Result` return
is theatre.

### `DaemonBuilder::default()` *(v1.1.0+)*

Equivalent to `Daemon::new()`. Enables `Default`-bound generics.

```rust
use proc_daemon::DaemonBuilder;

let builder = DaemonBuilder::default();
# let _ = builder;
```

---

## `DaemonBuilder`

Fluent builder. Every method takes `self` and returns `Self` so they
chain. Methods are documented in declaration order below.

### Subsystem registration

#### `with_task<F, Fut>(name: &str, task_fn: F) -> Self`

Add a function-style subsystem. The function receives a
`ShutdownHandle` and returns an awaitable `Result<()>`.

- `name`: human-readable identifier, used in logs and events.
- `task_fn`: any `Fn(ShutdownHandle) -> impl Future<Output = Result<()>> + Send + 'static`.

```rust
use proc_daemon::{Daemon, ShutdownHandle};
use std::time::Duration;

async fn metrics_pump(mut shutdown: ShutdownHandle) -> proc_daemon::Result<()> {
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            () = tokio::time::sleep(Duration::from_secs(10)) => {
                // emit metrics
            }
        }
    }
    Ok(())
}

# async fn run() -> proc_daemon::Result<()> {
Daemon::new()
    .with_task("metrics_pump", metrics_pump)
    .run()
    .await
# }
```

#### `with_subsystem<S>(subsystem: S) -> Self where S: Subsystem`

Add a trait-implementing subsystem. Use when you need health checks,
custom restart policy, or per-subsystem state.

```rust
use proc_daemon::{Daemon, RestartPolicy, ShutdownHandle, Subsystem};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

struct DatabaseSync {
    target: String,
}

impl Subsystem for DatabaseSync {
    fn run(
        &self,
        mut shutdown: ShutdownHandle,
    ) -> Pin<Box<dyn Future<Output = proc_daemon::Result<()>> + Send>> {
        let target = self.target.clone();
        Box::pin(async move {
            loop {
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    () = tokio::time::sleep(Duration::from_secs(5)) => {
                        // sync against `target`
                        let _ = &target;
                    }
                }
            }
            Ok(())
        })
    }

    fn name(&self) -> &str { "database_sync" }

    fn restart_policy(&self) -> RestartPolicy {
        RestartPolicy::ExponentialBackoff {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            max_attempts: 5,
        }
    }
}

# async fn run() -> proc_daemon::Result<()> {
Daemon::new()
    .with_subsystem(DatabaseSync { target: "primary".into() })
    .run()
    .await
# }
```

#### `with_subsystem_fn<F>(name: &str, register_fn: F) -> Self`

Low-level escape hatch. Hands you the `SubsystemManager` so you can
register through any path (`register`, `register_fn`,
`register_closure`).

```rust
use proc_daemon::Daemon;

# async fn run() -> proc_daemon::Result<()> {
Daemon::new()
    .with_subsystem_fn("worker", |mgr| {
        mgr.register_fn("worker", |mut shutdown| async move {
            shutdown.cancelled().await;
            Ok(())
        })
    })
    .run()
    .await
# }
```

### Signal configuration

#### `with_signal_config(config: SignalConfig) -> Self`

Customize which signals are handled. See [`signal`](#signal-handling).

#### `with_signals(sigterm: bool, sigint: bool) -> Self`

Shortcut for the two most common signals.

#### `without_signals() -> Self`

Disable signal handling entirely. The daemon will only shut down via
programmatic `daemon.shutdown()` or subsystem completion.

```rust
use proc_daemon::Daemon;

# async fn run() -> proc_daemon::Result<()> {
Daemon::new()
    .without_signals()
    .with_task("worker", |mut shutdown| async move {
        shutdown.cancelled().await;
        Ok(())
    })
    .run()
    .await
# }
```

### Config-watch

#### `with_config_path<P: Into<PathBuf>>(path: P) -> Self`

Override the path the `config-watch` feature watches. Defaults to
`Config::work_dir.join(DEFAULT_CONFIG_FILE)` or `./daemon.toml`.

### Terminal methods

#### `build(self) -> Result<Daemon>`

Construct the `Daemon` without starting it. Validates the config and
sets up the shutdown coordinator, subsystem manager, and signal
handler.

#### `async run(self) -> Result<()>`

`build()` + `Daemon::run()` in one call. The common shape.

---

## The `Subsystem` Trait

```rust,ignore
pub trait Subsystem: Send + Sync + 'static {
    fn run(
        &self,
        shutdown: ShutdownHandle,
    ) -> Pin<Box<dyn Future<Output = proc_daemon::Result<()>> + Send>>;

    fn name(&self) -> &str;

    fn health_check(&self) -> Option<Box<dyn Fn() -> bool + Send + Sync>> { None }
    fn restart_policy(&self) -> RestartPolicy { RestartPolicy::Never }
}
```

- `run`: produces the long-running future. Must respect `shutdown.cancelled()`.
- `name`: stable identifier; appears in logs, events, and metadata.
- `health_check`: optional; the daemon main loop polls these every
  `monitoring.health_check_interval_ms` when `monitoring.health_checks = true`.
- `restart_policy`: applies if the future returns `Err(_)` or panics.

The `Pin<Box<dyn Future>>` return shape is required for v1.x trait
objects. v2.0.0 will replace it with `async fn` in trait (tracked in
the v2 roadmap).

### Multiple ways to define a subsystem

#### Function

```rust
async fn worker(mut shutdown: proc_daemon::ShutdownHandle) -> proc_daemon::Result<()> {
    shutdown.cancelled().await;
    Ok(())
}
# fn _r() -> impl Fn(proc_daemon::ShutdownHandle) -> std::pin::Pin<Box<dyn std::future::Future<Output = proc_daemon::Result<()>> + Send>> {
#     |sh| Box::pin(worker(sh))
# }
```

#### Closure

```rust
let _closure = |mut shutdown: proc_daemon::ShutdownHandle| async move {
    shutdown.cancelled().await;
    proc_daemon::Result::Ok(())
};
```

#### Struct + impl

See `DatabaseSync` example above.

---

## `RestartPolicy`

```rust,ignore
pub enum RestartPolicy {
    Never,                                                                   // default
    Always,
    OnFailure,
    ExponentialBackoff {
        initial_delay: std::time::Duration,
        max_delay: std::time::Duration,
        max_attempts: u32,
    },
}
```

| Variant | Behavior |
|---|---|
| `Never` | Subsystem stays stopped after returning. |
| `Always` | Restart immediately whether returned `Ok` or `Err`. |
| `OnFailure` | Restart only if returned `Err(_)` or panicked. |
| `ExponentialBackoff { … }` | Restart on failure with capped exponential delays, up to `max_attempts`. |

```rust
use proc_daemon::RestartPolicy;
use std::time::Duration;

// Critical: never give up.
let _ = RestartPolicy::Always;

// Best-effort: tolerate transient errors.
let _ = RestartPolicy::OnFailure;

// Production default for restartable services:
let _ = RestartPolicy::ExponentialBackoff {
    initial_delay: Duration::from_secs(1),
    max_delay: Duration::from_secs(60),
    max_attempts: 5,
};
```

---

## `SubsystemState` & `SubsystemEvent`

### `SubsystemState`

```rust,ignore
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubsystemState {
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
    Restarting,
}
```

Read via `SubsystemMetadata.state` or queried through
`SubsystemManager::get_subsystem_metadata(id)`.

### `SubsystemMetadata`

```rust,ignore
pub struct SubsystemMetadata {
    pub id: SubsystemId,
    pub name: String,
    pub state: SubsystemState,
    pub registered_at: std::time::Instant,
    pub started_at: Option<std::time::Instant>,
    pub stopped_at: Option<std::time::Instant>,
    pub restart_count: u32,
    pub last_error: Option<String>,
    pub restart_policy: RestartPolicy,
}
```

### `SubsystemEvent`

Push-based notifications when `lockfree-coordination` is enabled.

```rust,ignore
pub enum SubsystemEvent {
    StateChanged {
        id: SubsystemId,
        name: String,
        state: SubsystemState,
        at: std::time::Instant,
    },
}
```

Activate via `manager.enable_events()`, then either poll
`try_next_event()` or take a cloned `Receiver` via `subscribe_events()`.

---

## `ShutdownHandle` & `ShutdownCoordinator`

### `ShutdownHandle` (held by subsystems)

```rust,ignore
impl ShutdownHandle {
    pub fn is_shutdown(&self) -> bool;
    pub async fn cancelled(&mut self);
    pub fn shutdown_reason(&self) -> Option<ShutdownReason>;
    pub fn shutdown_time(&self) -> Option<std::time::Instant>;
    pub fn is_forced(&self) -> bool;
    pub fn ready(&self);
    pub fn time_remaining(&self) -> Option<std::time::Duration>;
}
```

#### Typical use inside a subsystem

```rust
use proc_daemon::ShutdownHandle;
use std::time::Duration;

async fn worker(mut shutdown: ShutdownHandle) -> proc_daemon::Result<()> {
    loop {
        tokio::select! {
            () = shutdown.cancelled() => {
                tracing::info!("shutdown observed; cleaning up");
                break;
            }
            () = tokio::time::sleep(Duration::from_millis(100)) => {
                // do work
            }
        }
    }
    shutdown.ready();              // tell the coordinator we're clean
    Ok(())
}
```

#### Forced-shutdown awareness

```rust
use proc_daemon::ShutdownHandle;

async fn worker(mut shutdown: ShutdownHandle) -> proc_daemon::Result<()> {
    shutdown.cancelled().await;
    if shutdown.is_forced() {
        // Skip optional cleanup that might take too long.
    } else if let Some(remaining) = shutdown.time_remaining() {
        // Use the budget the coordinator is giving us.
        tracing::info!(?remaining, "graceful budget remaining");
    }
    shutdown.ready();
    Ok(())
}
```

### `ShutdownReason`

```rust,ignore
pub enum ShutdownReason {
    Signal(i32),          // OS signal (SIGTERM = 15, SIGINT = 2, etc.)
    Requested,            // programmatic daemon.shutdown()
    Error,                // a subsystem returned Err
    ResourceExhausted,    // emitted by future versions; reserved
    Forced,               // promotion: graceful timeout exceeded
}
```

### `ShutdownCoordinator` (held by the daemon)

```rust,ignore
impl ShutdownCoordinator {
    pub fn new(graceful_ms: u64, force_ms: u64, kill_ms: u64) -> Self;
    pub fn create_handle<S: Into<String>>(&self, subsystem_name: S) -> ShutdownHandle;

    pub fn initiate_shutdown(&self, reason: ShutdownReason) -> bool;
    pub fn is_shutdown(&self) -> bool;
    pub fn get_reason(&self) -> Option<ShutdownReason>;

    #[cfg(feature = "tokio")]
    pub async fn wait_initiated(&self);          // v1.1.0+: lock-free wait

    pub async fn wait_for_shutdown(&self) -> Result<()>;
    pub async fn wait_for_force_shutdown(&self) -> Result<()>;
    pub async fn wait_for_kill_shutdown(&self) -> Result<()>;
    pub fn get_stats(&self) -> ShutdownStats;
    pub fn update_timeouts(&self, graceful_ms: u64, force_ms: u64, kill_ms: u64);
}
```

#### `wait_initiated()` — new in v1.1.0

Resolves immediately when shutdown is initiated, without waiting
for subsystems to mark themselves ready. Use inside `tokio::select!`
to break out of polling loops. Used internally by the daemon main
loop to eliminate health-check-interval-bounded shutdown latency.

```rust
use proc_daemon::shutdown::ShutdownCoordinator;
use std::time::Duration;

# async fn example(coord: &ShutdownCoordinator) {
loop {
    tokio::select! {
        () = coord.wait_initiated() => break,
        () = tokio::time::sleep(Duration::from_secs(5)) => {
            // do periodic work
        }
    }
}
# }
```

---

## `Config` & `ConfigBuilder`

### Programmatic construction

```rust
use proc_daemon::{Config, LogLevel};
use std::time::Duration;

# fn example() -> proc_daemon::Result<Config> {
let config = Config::builder()
    .name("my-service")
    .log_level(LogLevel::Info)
    .json_logging(false)
    // Timeouts are validated against each other (graceful < force < kill).
    // The setters return `Result<Self>`; chain with `?`.
    .shutdown_timeout(Duration::from_secs(30))?
    .force_shutdown_timeout(Duration::from_secs(45))?
    .kill_timeout(Duration::from_secs(60))?
    .worker_threads(0)               // 0 = auto-detect via std::thread::available_parallelism()
    .enable_metrics(true)
    .hot_reload(true)
    .work_dir("/var/lib/my-service")
    .pid_file("/var/run/my-service.pid")
    .build()
# }
```

### From TOML file

```toml
# daemon.toml
name = "my-production-daemon"

[logging]
level = "info"
json  = false
color = true
file  = "/var/log/my-daemon.log"
max_file_size = 104857600   # 100 MiB
max_files     = 5

[shutdown]
graceful = 30000            # 30 s
force    = 45000            # 45 s
kill     = 60000            # 60 s

[performance]
worker_threads   = 0        # auto-detect
thread_pinning   = false
memory_pool_size = 1048576
numa_aware       = false
lock_free        = true

[monitoring]
enable_metrics             = true
metrics_interval_ms        = 1000
track_resources            = true
health_checks              = true
health_check_interval_ms   = 5000
```

```rust,no_run
use proc_daemon::Config;

# fn example() -> proc_daemon::Result<Config> {
Config::load_from_file("daemon.toml")
# }
```

### From environment

Environment variables override TOML, using the `DAEMON_` prefix and
double-underscore for nesting:

```bash
export DAEMON_NAME="my-daemon"
export DAEMON_LOGGING__LEVEL="debug"
export DAEMON_SHUTDOWN__GRACEFUL="60000"
export DAEMON_PERFORMANCE__WORKER_THREADS="16"
```

```rust,no_run
use proc_daemon::Config;

# fn example() -> proc_daemon::Result<Config> {
Config::load()                   // merges defaults + DEFAULT_CONFIG_FILE + env
# }
```

### Configuration sub-types

```rust,ignore
pub struct LogConfig {
    pub level: LogLevel,
    pub json: bool,
    pub color: bool,
    pub file: Option<std::path::PathBuf>,
    pub max_file_size: Option<u64>,
    pub max_files: Option<u32>,
}

pub struct ShutdownConfig {
    pub graceful: u64,
    pub force: u64,
    pub kill: u64,
}

pub struct PerformanceConfig {
    pub worker_threads: usize,
    pub thread_pinning: bool,
    pub memory_pool_size: usize,
    pub numa_aware: bool,
    pub lock_free: bool,
}

pub struct MonitoringConfig {
    pub enable_metrics: bool,
    pub metrics_interval_ms: u64,
    pub track_resources: bool,
    pub health_checks: bool,
    pub health_check_interval_ms: u64,
}

pub enum LogLevel { Trace, Debug, Info, Warn, Error }
```

### Live snapshots (config-watch)

When `config-watch` is enabled and `Config.hot_reload = true`, the
daemon installs a `notify` watcher and updates an `ArcSwap<Config>`
on every successful reload:

```rust,ignore
let snapshot = daemon.config_snapshot();   // Arc<Config>
let level = snapshot.logging.level;
```

`config_snapshot()` is feature-gated to `config-watch`.

---

## Error Handling

### `Result<T>` and `Error`

```rust,ignore
pub type Result<T> = std::result::Result<T, Error>;
```

`Error` is a `thiserror::Error` enum with these variants:

- `Config` — invalid or unparseable configuration.
- `Signal` — signal-handling setup or invocation failure.
- `Shutdown` — coordinator failure or timeout exceeded.
- `Subsystem` — registration, start, or stop failure.
- `Io` — file/path errors.
- `ResourceExhausted` — soft-limit or hard-limit hit.
- `Timeout` — explicit timeout exceeded.
- `InvalidState` — illegal state transition or value.
- `Platform` — feature unavailable on this OS.

Every variant carries an `ErrorCode` (stable numeric identifier in the
1000–10999 range) for metrics and structured logging:

```rust
use proc_daemon::Error;
# fn example(err: Error) {
match err {
    Error::Config { code, message, .. } => {
        eprintln!("config error {code}: {message}");
    }
    Error::Subsystem { code, name, message, .. } => {
        eprintln!("subsystem '{name}' failed [{code}]: {message}");
    }
    _ => {}
}
# }
```

### Constructors

Each variant has a short constructor:

```rust
use proc_daemon::Error;

let _ = Error::config("invalid config");
let _ = Error::io_with_source("read failed", std::io::Error::other("eof"));
let _ = Error::subsystem("worker", "panicked");
let _ = Error::timeout("flush", 5_000);
```

### `ErrorCode`

Categorized integer codes:

| Range | Category |
|---|---|
| 1000–1999 | Config |
| 2000–2999 | Signal |
| 3000–3999 | Shutdown |
| 4000–4999 | Subsystem |
| 5000–5999 | I/O |
| 6000–6999 | Runtime |
| 7000–7999 | Resource exhaustion |
| 8000–8999 | Timeout |
| 9000–9999 | State |
| 10000–10999 | Platform |
| 99999 | Unknown |

### `serde` feature

Enable `features = ["serde"]` to serialize `Error` and `ErrorCode`
into structured logs or external transport. `source: dyn Error`
fields are skipped during serialization.

### `backtrace` feature

Enable `features = ["backtrace"]` to attach backtraces to errors
created via `BacktraceError::new(...)` and
`BacktraceError::with_source(...)`. Backtrace capture follows
`RUST_BACKTRACE`.

---

## Optional modules

### `metrics` feature

Built-in counters / gauges / histograms with snapshot export.

```toml
[dependencies]
proc-daemon = { version = "1.1.1", features = ["metrics"] }
```

```rust,ignore
use proc_daemon::metrics::MetricsCollector;
use std::time::Duration;

let collector = MetricsCollector::new();

// Counters
collector.increment_counter("requests_total", 1);
collector.increment_counter("requests_total", 5);

// Gauges
collector.set_gauge("active_connections", 42);

// Histograms (raw + Duration helper)
collector.record_histogram("request_duration", Duration::from_millis(150));

// Snapshot at any time
let snapshot = collector.get_metrics();
println!("uptime: {:?}", snapshot.uptime);
```

Auto-scoped timing with `Timer`:

```rust,ignore
use proc_daemon::metrics::{MetricsCollector, Timer};
use std::sync::Arc;

let collector = Arc::new(MetricsCollector::new());
{
    let _timer = Timer::start("operation", Arc::clone(&collector));
    // ... do work ...
}                                    // duration recorded on drop
```

### `resources`

Cross-platform per-process memory / CPU / thread sampling. Always
available (no feature gate).

```rust
use proc_daemon::resources::ResourceTracker;
use std::time::Duration;

let tracker = ResourceTracker::new(Duration::from_secs(1));
let usage = tracker.current_usage();
println!("memory: {:.2} MB, cpu: {:.1}%", usage.memory_mb(), usage.cpu_percent());
# let _ = tracker;
```

With alerting:

```rust,no_run
use proc_daemon::resources::ResourceTracker;
use std::time::Duration;

let tracker = ResourceTracker::new(Duration::from_secs(5));
let _ = tracker;
// Configure soft-limit + alert handler via the tracker's builder
// methods (see source for the full API). Alerts fire when the soft
// memory limit is exceeded; the handler is `Arc<dyn Fn(Alert)>`.
```

### Signal handling

The default `SignalHandler` listens for SIGTERM, SIGINT, SIGQUIT, and
SIGHUP on Unix; Ctrl-C and console events on Windows.

For customization, build a `SignalConfig`:

```rust
use proc_daemon::signal::SignalConfig;

let signal_config = SignalConfig::new()
    .with_sighup()                       // enable SIGHUP
    .with_sigusr1()                      // enable SIGUSR1
    .without_sigint()                    // disable SIGINT
    .with_custom_handler(12, "Custom signal");
# let _ = signal_config;
```

Pass to the builder:

```rust
use proc_daemon::{signal::SignalConfig, Daemon};

# async fn example(signal_config: SignalConfig) -> proc_daemon::Result<()> {
Daemon::new()
    .with_signal_config(signal_config)
    .run()
    .await
# }
```

Pretty-print a signal number:

```rust
let _ = proc_daemon::signal::signal_description(15);   // "SIGTERM"
```

### Instance locking

Prevent multiple daemons from running with the same PID file:

```rust,no_run
use proc_daemon::lock::InstanceLock;
use std::path::Path;

let _lock = InstanceLock::acquire(Path::new("/var/run/my-daemon.pid"))
    .expect("another instance is running");
```

Drop releases the lock and removes the file.

### Object pools

Reusable buffers to reduce allocation pressure in hot paths:

```rust,ignore
use proc_daemon::{StringPool, VecPool};

let strings = StringPool::new(/*initial=*/ 16, /*max=*/ 128, /*cap=*/ 64);
{
    let mut s = strings.acquire();
    s.push_str("hello");
}                                        // returned to pool on drop

let buffers: VecPool<u8> = VecPool::new(8, 32, 1024);
{
    let mut buf = buffers.acquire();
    buf.extend_from_slice(b"payload");
}
```

### `ipc` feature

Unix-domain sockets on Unix, named pipes on Windows.

```rust,ignore
// Unix:
use proc_daemon::ipc::unix;
let listener = unix::bind("/tmp/my-daemon.sock").await?;
loop {
    let (_stream, _addr) = listener.accept().await?;
}
```

```rust,ignore
// Windows:
use proc_daemon::ipc::windows;
let server = windows::create_server(r"\\.\pipe\my-daemon")?;
windows::server_connect(&server).await?;
windows::echo_once(server).await?;
```

The Unix `bind` helper rejects existing non-socket files and
symlinks to avoid TOCTOU footguns; see `src/ipc.rs` for the full
security note.

### Profiling

`profiling` feature (Unix-only): CPU profile via `pprof`.

```rust,ignore
use proc_daemon::profiling::CpuProfiler;

let prof = CpuProfiler::start()?;
// ... workload ...
prof.stop_to_file("cpu.pb")?;
```

`heap-profiling` feature (cross-platform): heap profile via `dhat`.

```rust,ignore
use proc_daemon::profiling::heap::HeapProfiler;

let prof = HeapProfiler::start(Some("heap.json"))?;
// ... allocation-heavy workload ...
prof.stop();
```

On Windows the CPU profiler is unavailable (the feature is target-gated
to Unix in v1.0.1+). Heap profiling works on all platforms.

### Coordination primitives

Lock-free channel facade. Backed by `crossbeam-channel` when
`lockfree-coordination` is enabled, falling back to `std::sync::mpsc`
otherwise — same API in both cases.

```rust
use proc_daemon::coord::chan;

let (tx, rx) = chan::unbounded::<u32>();
tx.send(7).unwrap();
match chan::try_recv(&rx) {
    Ok(v) => println!("got {v}"),
    Err(_) => println!("empty"),
}
```

Subsystem events stream through the same channel when
`SubsystemManager::enable_events()` is called.

### High-resolution timing

Enable `high-res-timing` to access an ultra-fast monotonic clock
backed by `quanta` (RDTSC where available).

```rust,ignore
let t0 = proc_daemon::timing::now();
// ... measured work ...
let t1 = proc_daemon::timing::now();
let dt = t1.duration_since(t0);
println!("elapsed: {dt:?}");
```

### Scheduler hints

Enable `scheduler-hints` to expose two best-effort tuning hooks:

```rust,ignore
proc_daemon::scheduler::apply_process_hints(config);
proc_daemon::scheduler::apply_runtime_hints();
```

With `scheduler-hints-unix` on Linux, `apply_process_hints` attempts
a `renice -5` (no-op if it lacks privileges) and `apply_runtime_hints`
sets a permissive CPU affinity mask. On Windows / macOS both calls
are no-ops.

---

## Feature Flags

| Feature | Description | Default | Platform |
|---|---|---|---|
| `tokio` | Tokio runtime (recommended). | ✅ | all |
| `toml` | TOML config loader (used by `Config::load_from_file`). | ✅ | all |
| `async-std` | async-std runtime (legacy; removal in v2.0.0). | ❌ | all |
| `metrics` | Counters / gauges / histograms via `MetricsCollector`. | ❌ | all |
| `console` | Colored terminal output for selected helpers. | ❌ | all |
| `json-logs` | JSON-formatted `tracing` output. | ❌ | all |
| `config-watch` | Live-reload `Config` on file change (`notify`). | ❌ | all |
| `ipc` | Unix-socket / named-pipe IPC scaffold. | ❌ | all |
| `mmap-config` | Memory-mapped fast-path for TOML loading. | ❌ | all |
| `mimalloc` | Switch global allocator to mimalloc. | ❌ | all |
| `high-res-timing` | `quanta`-backed `proc_daemon::timing`. | ❌ | all |
| `scheduler-hints` | Best-effort scheduler hooks. | ❌ | all |
| `scheduler-hints-unix` | Activates `nix/resource` for renice/affinity. | ❌ | Unix |
| `lockfree-coordination` | `crossbeam-channel` backend + subsystem events. | ❌ | all |
| `profiling` | CPU profiler (`pprof`, Unix-only). | ❌ | Unix |
| `heap-profiling` | Heap profiler (`dhat`). | ❌ | all |
| `backtrace` | Backtrace capture on error creation. | ❌ | all |
| `serde` | Serialize `Error`/`ErrorCode` (used by JSON logs and IPC). | ❌ | all |
| `windows-monitoring` | Win32 ToolHelp-based process monitoring. | ❌ | Windows |
| `full` | `tokio`+`metrics`+`console`+`json-logs`+`config-watch`+`ipc`. | ❌ | all |

---

## Platform Support

| Platform | Status |
|---|---|
| Linux x86_64 | first-class |
| Linux aarch64 | first-class |
| macOS x86_64 / aarch64 | first-class |
| Windows x86_64 (MSVC) | first-class (with `windows-monitoring` for Win32 stats) |
| FreeBSD / OpenBSD | should work via the Unix path; not regularly tested |

Notes:

- `profiling` is Unix-only (target-gated). On Windows the feature is
  inert — no compile error, but `CpuProfiler` is not exported. Use
  `heap-profiling` (dhat) cross-platform or a Windows-native profiler.
- `scheduler-hints-unix` is a no-op on non-Unix.
- `async-std` is marked legacy and slated for removal in v2.0.0.

---

## Best Practices

1. **Always `select!` against `cancelled()` in long-running futures.**
   The daemon main loop already does this since v1.1.0, but every
   subsystem with its own sleep/poll loop must too — otherwise
   shutdown latency is bounded by your interval.

2. **Mark `ready()` after cleanup.** The coordinator can't tell the
   difference between "still working" and "done cleaning up" without
   it; you'll be force-aborted instead of getting credit for being
   tidy.

3. **Use `RestartPolicy::ExponentialBackoff` for network-facing
   subsystems.** `Always` will hot-loop on persistent failures;
   `Never` will silently die.

4. **Set `force_shutdown_timeout > shutdown_timeout > 0` and
   `kill_timeout > force_shutdown_timeout`.** The builder validates
   this; mis-ordered timeouts fail at `build()`.

5. **Prefer `Daemon::new()` over `Daemon::builder(Config::default())`
   or `Daemon::with_defaults()?`.** They're equivalent; `new()` is
   shorter and infallible (v1.1.0+).

6. **Enable `mimalloc` for allocation-heavy workloads.** It's a
   one-line opt-in (`features = ["mimalloc"]`) and typically wins
   5–15% on throughput in services that allocate a lot per request.

7. **Treat `Daemon::run()` as the last call in `main()`.** It owns
   the runtime for the process lifetime. If you need to share state
   with subsystems, capture it before calling `run`.

8. **Subscribe to `SubsystemEvent` rather than polling
   `get_stats()`** when you need reactivity. Polling is fine for
   periodic introspection (every few seconds); events are required
   for sub-second latency.

---

**Compatibility:** This document is current as of **v1.1.1**.
Earlier APIs that have been replaced or augmented are still listed
(e.g., `Daemon::with_defaults`), with the preferred form noted.
v2.0.0 will introduce breaking changes; see the local
`.dev/V2-ROADMAP.md` for the plan.
