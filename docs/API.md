<div align="center">
    <img width="108xpx" src="../media/proc-rs-orange.svg" alt="High-Performance Process EcoSystem for Rust">
    <h1>
        <strong>Process Daemon</strong>
        <sup><br><sub>API REFERENCE</sub><br></sup>
    </h1>
</div>

**Status:** ✅ v1.0.0 Stable Release — Production-ready framework for high-performance daemon services.

## Table of Contents

- [Installation](#installation)
- [Core Concepts](#core-concepts)
- [Daemon Builder](#daemon-builder)
- [Subsystems](#subsystems)
- [Configuration](#configuration)
- [Feature Flags](#feature-flags)
- [Advanced Features](#advanced-features)
  - [Metrics Collection](#metrics-collection)
  - [Resource Tracking](#resource-tracking)
  - [Profiling](#profiling)
  - [Configuration Hot-Reload](#configuration-hot-reload)
  - [Coordination & Events](#coordination--events)
  - [High-Resolution Timing](#high-resolution-timing)
  - [Scheduler Hints](#scheduler-hints)
- [Module Structure](#module-structure)
- [Error Handling](#error-handling)
- [Platform Support](#platform-support)
- [Concurrency & Safety](#concurrency--safety)
- [Best Practices](#best-practices)
- [Examples](#examples)

## Installation

### Install Manually

```toml
[dependencies]
proc-daemon = "1.0.0"
```

### With Specific Features

```toml
[dependencies]
proc-daemon = { version = "1.0.0", features = ["tokio", "metrics", "high-res-timing"] }
```

## Core Concepts

### Daemon

The `Daemon` is the main entry point for building resilient services. It:
- Manages concurrent subsystems
- Handles cross-platform signal handling
- Coordinates graceful shutdown
- Supports configuration hot-reload
- Collects optional metrics

### Subsystems

A `Subsystem` is an independently managed, long-running task with:
- Automatic restart policies (Never, Always, OnFailure, ExponentialBackoff)
- Optional health checks
- Lifecycle monitoring (Starting → Running → Stopping → Stopped)
- Coordinated shutdown support

### ShutdownHandle

Every subsystem/task receives a `ShutdownHandle` for graceful termination:
- `shutdown.cancelled()` — Async signal when shutdown begins
- `shutdown.is_shutdown()` — Synchronous shutdown check
- Facilitates `tokio::select!` patterns for clean cancellation

## Daemon Builder

The fluent builder pattern makes construction intuitive:

```rust
use proc_daemon::{Daemon, Config, RestartPolicy};
use std::time::Duration;

let config = Config::builder()
    .name("my-daemon")
    .shutdown_timeout(Duration::from_secs(30))
    .enable_metrics(true)
    .build()?;

let daemon = Daemon::builder(config)
    .with_task("main_service", |shutdown| {
        Box::pin(async move {
            // Your async service code
            Ok(())
        })
    })
    .run()
    .await?;
```

### API Methods

- **`with_task(name, fn)`** — Register a simple async function
- **`with_subsystem(subsystem)`** — Register a custom `Subsystem` impl
- **`with_subsystem_fn(name, fn)`** — Register a closure subsystem
- **`with_signal_config(config)`** — Customize signal handling
- **`with_pid_file(path)`** — Write daemon PID to file on startup
- **`run()`** — Start the daemon (returns handle)

## Subsystems

### Simple Subsystem

```rust
use proc_daemon::{Subsystem, ShutdownHandle, RestartPolicy, Result};
use std::pin::Pin;
use std::future::Future;
use std::time::Duration;

struct MyService {
    name: String,
}

impl Subsystem for MyService {
    fn run(&self, mut shutdown: ShutdownHandle) 
        -> Pin<Box<dyn Future<Output = Result<()>> + Send>> 
    {
        let name = self.name.clone();
        Box::pin(async move {
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => {
                        tracing::info!("service {} shutting down", name);
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        tracing::info!("service {} working", name);
                    }
                }
            }
            Ok(())
        })
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn restart_policy(&self) -> RestartPolicy {
        RestartPolicy::OnFailure
    }

    fn health_check(&self) -> Option<Box<dyn Fn() -> bool + Send + Sync>> {
        Some(Box::new(|| {
            // Simple health check
            true
        }))
    }
}
```

### Using with Daemon

```rust
let daemon = Daemon::builder(config)
    .with_subsystem(MyService { name: "worker".into() })
    .run()
    .await?;
```

## Configuration

### Programmatic Configuration

```rust
use proc_daemon::{Config, LogLevel};
use std::time::Duration;

let config = Config::builder()
    .name("production-daemon")
    .log_level(LogLevel::Info)
    .json_logging(true)
    .shutdown_timeout(Duration::from_secs(30))
    .worker_threads(8)
    .enable_metrics(true)
    .hot_reload(true)
    .build()?;
```

### File-Based Configuration (TOML)

Create `daemon.toml`:

```toml
name = "my-production-daemon"

[logging]
level = "info"
json = false
color = true
file = "/var/log/my-daemon.log"
max_file_size = 104857600  # 100MB
max_files = 5

[shutdown]
timeout_ms = 30000
force_timeout_ms = 45000
kill_timeout_ms = 60000

[performance]
worker_threads = 0          # Auto-detect
thread_pinning = false
memory_pool_size = 1048576  # 1MB
numa_aware = false
lock_free = true

[monitoring]
enable_metrics = true
metrics_interval_ms = 1000
track_resources = true
health_checks = true
health_check_interval_ms = 5000
```

Load configuration:

```rust
use proc_daemon::Config;
let config = Config::load_from_file("daemon.toml")?;
```

### Environment Variables

All configuration can be overridden via environment variables with `DAEMON_` prefix:

```bash
export DAEMON_NAME="env-daemon"
export DAEMON_LOGGING_LEVEL="debug"
export DAEMON_SHUTDOWN_TIMEOUT_MS="60000"
export DAEMON_PERFORMANCE_WORKER_THREADS="16"
```

## Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `tokio` | Tokio runtime support (primary) | ✅ |
| `async-std` | async-std runtime (unmaintained; best-effort legacy) | ❌ |
| `metrics` | Performance metrics collection and reporting | ❌ |
| `console` | Enhanced console output | ❌ |
| `json-logs` | JSON structured logging | ❌ |
| `config-watch` | Configuration hot-reloading via file watcher | ❌ |
| `mmap-config` | Memory-mapped config loading (affects TOML load speed) | ❌ |
| `mimalloc` | Use mimalloc as global allocator | ❌ |
| `high-res-timing` | High-resolution timing via `quanta` | ❌ |
| `scheduler-hints` | Process-level scheduler tuning hooks | ❌ |
| `scheduler-hints-unix` | Unix-specific `renice` integration (requires elevation) | ❌ |
| `lockfree-coordination` | Lock-free MPMC channels and subsystem events | ❌ |
| `profiling` | CPU profiling via `pprof` (Unix-preferred) | ❌ |
| `heap-profiling` | Heap profiling via `dhat` | ❌ |
| `full` | Enable all features | ❌ |

## Advanced Features

### Metrics Collection

When `metrics` feature is enabled:

```toml
[dependencies]
proc-daemon = { version = "1.0.0", features = ["metrics"] }
```

```rust
#[cfg(feature = "metrics")]
{
    use proc_daemon::metrics::MetricsCollector;
    
    let collector = MetricsCollector::new();
    
    // Counters
    collector.increment_counter("requests_total", 1);
    
    // Gauges
    collector.set_gauge("active_connections", 42);
    
    // Histograms
    use std::time::Duration;
    collector.record_histogram("request_latency_ms", Duration::from_millis(150));
    
    // Snapshot
    let snapshot = collector.get_metrics();
    println!("Uptime: {:?}", snapshot.uptime);
    println!("Worker threads: {}", snapshot.worker_count);
}
```

### Resource Tracking

```rust
#[cfg(feature = "metrics")]
{
    use proc_daemon::resources::ResourceTracker;
    use std::sync::Arc;
    
    let tracker = ResourceTracker::new();
    
    // Start tracking in background
    let tracker_clone = tracker.clone();
    tokio::spawn(async move {
        tracker_clone.start().await;
    });
    
    // Read current resource usage
    let current = tracker.current_usage();
    println!("Memory: {} MB", current.memory_mb);
    println!("CPU: {} %", current.cpu_percent);
    println!("Threads: {}", current.thread_count);
}
```

### Profiling

#### CPU Profiling

When `profiling` feature is enabled:

```toml
[dependencies]
proc-daemon = { version = "1.0.0", features = ["profiling"] }
```

```rust
#[cfg(feature = "profiling")]
{
    use proc_daemon::profiling::CpuProfiler;
    
    let cpu = CpuProfiler::start()?;
    // ... run workload ...
    cpu.stop_to_file("cpu_profile.pb")?;
    
    // Analyze with: go tool pprof cpu_profile.pb
}
```

#### Heap Profiling

When `heap-profiling` feature is enabled:

```toml
[dependencies]
proc-daemon = { version = "1.0.0", features = ["heap-profiling"] }
```

```rust
#[cfg(feature = "heap-profiling")]
{
    use proc_daemon::profiling::heap::HeapProfiler;
    
    let hp = HeapProfiler::start(Some("heap_profile.json"))?;
    // ... run workload ...
    hp.stop();
    
    // Analyze with dhat viewer
}
```

### Configuration Hot-Reload

When `config-watch` feature is enabled:

```toml
[dependencies]
proc-daemon = { version = "1.0.0", features = ["config-watch", "toml", "mmap-config"] }
```

```rust
#[cfg(feature = "config-watch")]
{
    use proc_daemon::Config;
    
    let config = Config::builder()
        .name("hot-reload-daemon")
        .hot_reload(true)  // Enable file watcher
        .build()?;
    
    let daemon = Daemon::builder(config).run().await?;
    
    // Later: read live config snapshot
    let live = daemon.config_snapshot();
    println!("Current name: {}", live.name);
    println!("Current log level: {:?}", live.logging.level);
}
```

When `daemon.toml` is modified, the daemon automatically picks up changes—no restart required.

Run the example:

```bash
cargo run --example hot_reload --features "tokio config-watch toml mmap-config"
```

### Coordination & Events

When `lockfree-coordination` feature is enabled:

```toml
[dependencies]
proc-daemon = { version = "1.0.0", features = ["lockfree-coordination"] }
```

#### Channel Facade

Simple MPMC channel wrapper (always available, auto-selects backend):

```rust
use proc_daemon::coord::chan;

let (tx, rx) = chan::unbounded::<String>();
tx.send("hello".into()).ok();

// Non-blocking receive
if let Ok(msg) = chan::try_recv(&rx) {
    println!("Got: {}", msg);
}
```

#### Subsystem Events

```rust
#[cfg(feature = "lockfree-coordination")]
{
    use proc_daemon::SubsystemManager;
    
    let manager = SubsystemManager::new();
    manager.enable_events();
    
    // Poll events without blocking
    if let Some(ev) = manager.try_next_event() {
        match ev {
            proc_daemon::subsystem::SubsystemEvent::StateChanged { id, name, state, at } => {
                println!("[{:?}] {} -> {:?}", at, name, state);
            }
        }
    }
}
```

### High-Resolution Timing

When `high-res-timing` feature is enabled:

```toml
[dependencies]
proc-daemon = { version = "1.0.0", features = ["high-res-timing"] }
```

```rust
#[cfg(feature = "high-res-timing")]
{
    use proc_daemon::timing;
    
    let t0 = timing::now();
    // ... do work ...
    let t1 = timing::now();
    let elapsed = t1.duration_since(t0);
    
    println!("Elapsed: {} nanoseconds", elapsed.as_nanos());
}
```

The `quanta` clock provides nanosecond precision with minimal overhead.

### Scheduler Hints

When `scheduler-hints` feature is enabled (with optional `scheduler-hints-unix`):

```toml
[dependencies]
proc-daemon = { version = "1.0.0", features = ["scheduler-hints", "scheduler-hints-unix"] }
```

```rust
#[cfg(feature = "scheduler-hints")]
{
    use proc_daemon::scheduler;
    
    let config = Config::new()?;
    
    // Apply process-level hints (best-effort)
    scheduler::apply_process_hints(&config);
    
    // Apply runtime-level hints (currently noop)
    scheduler::apply_runtime_hints();
}
```

**Note:** `scheduler-hints-unix` uses `renice` and requires elevated privileges. Failures are logged at debug level and don't interrupt execution.

## Module Structure

### Public Modules

- **`Daemon`** — Main daemon builder and runner
- **`Config`** — Configuration management (programmatic, file-based, environment)
- **`Subsystem`** — Subsystem trait and lifecycle management
- **`ShutdownHandle`** — Graceful shutdown signaling
- **`signal`** — Cross-platform signal handling
- **`lock`** — Synchronization utilities
- **`resources`** — (Optional) Resource tracking and monitoring
- **`metrics`** — (Optional) Metrics collection
- **`profiling`** — (Optional) CPU and heap profiling
- **`coord`** — (Optional) Lock-free coordination primitives
- **`timing`** — (Optional) High-resolution timing
- **`scheduler`** — (Optional) Scheduler tuning hints
- **`ipc`** — (Optional) Inter-process communication

## Error Handling

All fallible operations return `proc_daemon::Result<T>`, which aliases `Result<T, proc_daemon::Error>`.

```rust
use proc_daemon::{Daemon, Config, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::new()?;
    Daemon::builder(config).run().await?;
    Ok(())
}
```

Error types include:
- **`ConfigError`** — Configuration parsing or validation failure
- **`IoError`** — File I/O, signal registration, PID file errors
- **`SubsystemError`** — Subsystem lifecycle failures
- **`ShutdownError`** — Shutdown coordination timeout or abort
- **`ProfilingError`** — Profiler initialization or output

## Platform Support

### Linux/macOS (Unix)

- Full signal handling (SIGTERM, SIGINT, SIGQUIT, SIGHUP)
- CPU and heap profiling (if features enabled)
- Resource tracking via `/proc` filesystem
- Scheduler hints via `renice`

### Windows

- Console event handling (CTRL+C, CTRL+BREAK)
- Resource monitoring via Windows APIs
- Signal handling emulation for portability
- pprof profiling has known build incompatibilities; consider alternative profilers

## Concurrency & Safety

- **Lock-Free Performance**: Hot paths use atomic operations with optimized orderings
- **Poison Protection**: RwLock poison is documented; panics are guarded
- **Memory Pooling**: Pre-allocated pools reduce GC pressure
- **Arc Sharing**: Zero-copy configuration and state sharing
- **100k+ Concurrent Tasks**: Tested with high concurrency workloads

## Best Practices

1. **Use `tokio::select!` in subsystems** for clean shutdown handling
2. **Enable metrics in production** for observability
3. **Set appropriate shutdown timeouts** for your workload
4. **Use health checks** for automatic failure detection
5. **Log shutdown events** for operational clarity
6. **Test with `--all-features`** to catch feature-gate issues
7. **Monitor resource usage** to detect leaks early

## Examples

Runnable examples are available in the `examples/` directory:

```bash
# Simple daemon
cargo run --example simple --features tokio

# Multi-subsystem daemon
cargo run --example comprehensive --features tokio

# Configuration hot-reload
cargo run --example hot_reload --features "tokio config-watch toml"

# Metrics server
cargo run --example metrics_server --features "tokio metrics"
```

See the [main README](../README.md) and [examples directory](../examples/) for detailed examples.

---

<div align="center">
  <br>
  <h2></h2>
  <sup>COPYRIGHT <small>&copy;</small> 2026 <strong>JAMES GOBER.</strong></sup>
</div>
