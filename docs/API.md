<div align="center">
    <img width="108xpx" src="../media/proc-rs-orange.svg" alt="High-Performance Process EcoSystem for Rust">
    <h1>
        <strong>Process Daemon</strong>
        <sup><br><sub>API REFERENCE</sub><br></sup>
    </h1>
</div>
<br>

<br>

## Table of Contents
- [Installation](#installation)

<br><br>

## Installation

### Install Manually
```toml
[dependencies]
proc-daemon = "1.0.0"
```

Subscribe to events (lock-free backend):

```rust
// After calling manager.enable_events()
#[cfg(feature = "lockfree-coordination")]
if let Some(rx) = manager.subscribe_events() {
    use proc_daemon::coord::chan;
    if let Ok(ev) = chan::try_recv(&rx) {
        match ev {
            proc_daemon::subsystem::SubsystemEvent::StateChanged { id, name, state, at } => {
                println!("[{:?}] {}({}) -> {:?}", at, name, id, state);
            }
        }
    }
}
```

<br>

## Profiling

### CPU Profiling (`profiling` feature)

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
    cpu.stop_to_file("cpu_profile.pb")?; // protobuf for `go tool pprof`
}
```

### Heap Profiling (`heap-profiling` feature)

```toml
[dependencies]
proc-daemon = { version = "1.0.0", features = ["heap-profiling"] }
```

```rust
#[cfg(feature = "heap-profiling")]
{
    use proc_daemon::profiling::heap::HeapProfiler;
    // Optionally set output target; defaults to DHAT's standard if not provided
    let hp = HeapProfiler::start(Some("dhat-allocs.json"))?;
    // ... run workload ...
    hp.stop(); // or let it drop at end of scope
}
```

<br>

## Configuration Hot-Reload

- Requires features: `config-watch` (optionally `mmap-config`, `toml`).
- When `Config.hot_reload = true`, the daemon starts a file watcher for `daemon.toml` and maintains a live snapshot.
- Read the latest config via `Daemon::config_snapshot()`.

Example (Tokio):

```rust
#[cfg(all(feature = "tokio", feature = "config-watch"))]
{
    use proc_daemon::{Config, Daemon};
    let daemon = Daemon::builder(
        Config::builder().name("hot").hot_reload(true).build()?
    ).build()?;

    let live = daemon.config_snapshot();
    println!("name={} json={}", live.name, live.is_json_logging());
}
```

Run the full example:

```bash
cargo run --example hot_reload --features "tokio config-watch toml mmap-config"
```

### Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `tokio` | Tokio runtime support | ✅ |
| `async-std` | async-std runtime support | ❌ |
| `metrics` | Performance metrics collection | ❌ |
| `console` | Enhanced console output | ❌ |
| `json-logs` | JSON structured logging | ❌ |
| `config-watch` | Configuration hot-reloading | ❌ |
| `mmap-config` | Memory-mapped config file loading (TOML fast-path, safe fallback) | ❌ |
| `mimalloc` | Use mimalloc as global allocator | ❌ |
| `high-res-timing` | High-resolution timing via `quanta` | ❌ |
| `scheduler-hints` | Enable scheduler tuning hooks (no-op by default) | ❌ |
| `scheduler-hints-unix` | Best-effort Unix niceness adjustment (uses `renice`; no-op without privileges) | ❌ |
| `lockfree-coordination` | Lock-free coordination/events via crossbeam-channel | ❌ |
| `profiling` | Optional CPU profiling via `pprof` | ❌ |
| `heap-profiling` | Optional heap profiling via `dhat` | ❌ |
| `full` | All features enabled | ❌ |

Note: `async-std` is discontinued upstream; support here is best-effort and intended for existing users.

Example enabling features:

```toml
[dependencies]
proc-daemon = { version = "1.0.0", features = ["tokio", "metrics", "high-res-timing"] }
```

<br>


## Coordination and Events

Enable the `lockfree-coordination` feature to use a lock-free MPMC channel for subsystem coordination and event polling.

```toml
[dependencies]
proc-daemon = { version = "1.0.0", features = ["lockfree-coordination"] }
```

Channel facade (always available):

```rust
use proc_daemon::coord::chan;

let (tx, rx) = chan::unbounded::<u32>();
tx.send(42).unwrap();
let val = chan::try_recv(&rx).ok(); // Non-blocking
```

Subsystem events (opt-in at runtime):

```rust
// manager: &proc_daemon::SubsystemManager
manager.enable_events();

if let Some(ev) = manager.try_next_event() {
    match ev {
        proc_daemon::SubsystemEvent::StateChanged { id, name, state, at } => {
            println!("[{:?}] {}({}) -> {:?}", at, name, id, state);
        }
    }
}
```


<!--
:: COPYRIGHT
============================================================================ -->
<div align="center">
  <br>
  <h2></h2>
  <sup>COPYRIGHT <small>&copy;</small> 2025 <strong>JAMES GOBER.</strong></sup>
</div>