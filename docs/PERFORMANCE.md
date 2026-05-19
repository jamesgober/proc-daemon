<div align="center">
    <img width="120px" height="auto" src="https://raw.githubusercontent.com/jamesgober/jamesgober/main/media/icons/hexagon-3.svg" alt="Triple Hexagon">
    <h1>
        <strong>Process Daemon</strong>
        <sup>
            <br>
            <sub>PERFORMANCE</sub>
            <br>
        </sup>
    </h1>
</div>

[Home](../README.md)

## Quick Summary

Typical performance characteristics (Criterion, release, v1.0.0
baseline retained from the published bench-suite run):

- **Daemon creation**: ~2.14 μs per init
- **Subsystem registration** (3 tasks): ~3.76 μs per build
- **Config build**: ~80.6 ns per build
- **Shutdown coordination** (single quick task): ~2.87 μs per cycle
- **Error creation**: ~31.2 ns per error
- **Error chain construction**: ~137 ns per chain

v1.1.0 additionally lands several non-benchmarked improvements that
show up in real workloads rather than micro-benches — see
[v1.1.0 changes](#v110-changes-non-benchmarked) below.

> **Note:** Benchmarks were measured on 2026-02-23 on Windows x86_64.
> ±5–10% variance is normal between runs. Re-running on Linux x86_64
> typically shows slightly lower numbers due to lower syscall overhead.

## Benchmark Methodology

All benchmarks are generated using [Criterion.rs](https://bheisler.github.io/criterion.rs/book/):

**Test Configuration:**

- **Compiler**: Rust 1.82.0+, release mode with debuginfo
- **Runtime**: Tokio (async runtime)
- **Features**: `tokio` (core) only; no metrics collection
- **Platform**: Windows x86_64 (benchmarks may vary on Linux/macOS due to syscall overhead)
- **Warmup**: 3 seconds per benchmark
- **Samples**: 100 measurements per benchmark
- **Iterations**: Auto-scaled by Criterion to achieve stable measurements

**Interpretation Guidance:**

- Results are median values; see [Version Performance History](#version-performance-history) for full timing ranges.
- ±5–10% variance is normal between runs on the same machine.
- Larger variance may occur with system thermal throttling or background load.
- Actual latency in production may differ based on workload complexity and subsystem count.

### Memory Usage

- **Base daemon**: ~1–2 MB
- **Per subsystem**: ~4–8 KB
- **Configuration**: ~1–4 KB
- **Signal handling**: ~512 B

<!-- PERFORMANCE DATA -->
## Core Performance Metrics

| Metric | Result (median) | Comparison | Improvement |
|--------|------------------|------------|-------------|
| Daemon creation | 2.14 μs | 9.0 μs (v0.9.0) | ≈76% faster |
| Subsystem registration (3 tasks) | 3.76 μs | 12.9 μs (v0.9.0) | ≈71% faster |
| Config build | 80.6 ns | 234 ns (v0.9.0) | ≈66% faster |
| Shutdown coordination (1 task) | 2.87 μs | 10.2 μs (v0.9.0) | ≈72% faster |
| Error creation | 31.2 ns | 71.7 ns (v0.9.0) | ≈57% faster |
| Error cause chain | 137 ns | 539 ns (v0.9.0) | ≈75% faster |

<br>

## v1.1.0 changes (non-benchmarked)

v1.1.0 didn't move the headline micro-benchmark numbers above
significantly — those code paths were already optimized through the
v1.0.0 cycle. Instead it lands two real-workload improvements that
the existing bench suite doesn't cover, plus a contention-side win
that only matters under load:

### Shutdown latency: bounded by interval → bounded by signal

Previously, the daemon main loop slept the full
`monitoring.health_check_interval_ms` (default 30 s) between
shutdown-flag checks. A shutdown initiated 1 µs after the sleep
began had to wait the full interval before the loop noticed.

v1.1.0 races the sleep against a new
`ShutdownCoordinator::wait_initiated()` broadcast wait via
`tokio::select!`. **Observable shutdown latency is now bounded by
signal propagation, not by the configured interval** — typically
sub-millisecond. This change is observable in operations (kill
times, deploy times), not in Criterion.

### `parking_lot` migration completed

v1.0.0-RC.1's CHANGELOG advertised a `std::sync::Mutex` →
`parking_lot::Mutex` migration. That migration only covered
`pool.rs`, `shutdown.rs`, and `metrics.rs`. v1.1.0 completes it
across `subsystem.rs`, `resources.rs`, and `daemon.rs`'s rotating
log writer.

What changes:

- **Lock acquisition** is roughly 2–3× faster under contention
  (parking_lot's spin-then-park design vs the OS-direct
  `pthread_mutex` / `SRWLock` path).
- **No more lock poisoning**. `parking_lot::Mutex::lock()` returns
  the guard directly; this eliminates an entire class of latent
  failure modes inside subsystem metadata and resource history.
- **Smaller binary**: no `PoisonError` machinery for the converted
  call sites.

In Criterion micro-benchmarks this swap is barely visible (most
benches don't contend on these mutexes), but **per-log-line latency**
under high logging volume and **subsystem-metadata read latency**
under many concurrent stats queries both improve measurably in real
workloads.

### Allocations: stale doc comments + `.unwrap()` calls removed

24 `.lock().unwrap()` / `.read().unwrap()` / `.write().unwrap()`
patterns were removed when `parking_lot` was adopted. Each was a
tiny branch the compiler had to keep alive; their removal slightly
shrinks the instruction footprint of subsystem and resource code.
Not enough to register on the benchmarks, but real.

<br>

## Version Performance History

Results generated with Criterion, default configuration, Tokio
runtime, no metrics feature (unless noted).

| Version | Date | Daemon create | Subsystem reg (3) | Config build | Shutdown (1 task) | Error create | Error chain |
|---------|------|---------------|-------------------|--------------|-------------------|--------------|-------------|
| 1.1.1   | 2026-05-19 | (unchanged) | (unchanged) | (unchanged) | (unchanged) | (unchanged) | (unchanged) |
| 1.1.0   | 2026-05-19 | (unchanged) | (unchanged) | (unchanged) | (unchanged) | (unchanged) | (unchanged) |
| 1.0.1   | 2026-05-18 | (unchanged) | (unchanged) | (unchanged) | (unchanged) | (unchanged) | (unchanged) |
| 1.0.0   | 2026-02-23 | 2.14 μs | 3.76 μs | 80.6 ns | 2.87 μs | 31.2 ns | 137 ns |
| 0.9.0   | 2025-08-26 | 9.0 μs  | 12.9 μs | 234 ns  | 10.2 μs | 71.7 ns | 539 ns |

v1.0.1 was a maintenance / audit patch with no perf-relevant code
changes; v1.1.0's gains land in shutdown responsiveness and
under-contention lock paths that this bench suite doesn't exercise
(see [v1.1.0 changes](#v110-changes-non-benchmarked) above).
v1.1.1 is a docs-only patch.

## Running Benchmarks Yourself

To run the benchmark suite on your own hardware:

```bash
# Run all benchmarks with Tokio
cargo bench --bench daemon_bench --features tokio

# Run benchmarks with metrics enabled (slightly slower)
cargo bench --bench daemon_bench --features "tokio metrics"

# Verbose output (per-sample measurements)
cargo bench --bench daemon_bench --features tokio -- --verbose
```

**Note on Windows:** some syscall-related benchmarks may show higher
latencies than Linux due to Windows kernel architecture differences.
This is expected and doesn't indicate a problem.

## Performance Optimization Tips

### For Your Application

1. **Enable `mimalloc` in production** if your daemon allocates heavily:
   ```toml
   proc-daemon = { version = "1.1.1", features = ["tokio", "mimalloc"] }
   ```

2. **Use `high-res-timing` for sub-microsecond operations**:
   ```toml
   proc-daemon = { version = "1.1.1", features = ["tokio", "high-res-timing"] }
   ```

3. **Profile your subsystems** using `--features profiling` (Unix-only;
   the feature is target-gated and is a no-op on Windows). Use
   `--features heap-profiling` for cross-platform heap profiling.

4. **Enable scheduler hints on Unix** for reduced context switching:
   ```toml
   proc-daemon = { version = "1.1.1", features = ["tokio", "scheduler-hints-unix"] }
   ```

5. **Use lock-free coordination** for high-frequency inter-subsystem messaging:
   ```toml
   proc-daemon = { version = "1.1.1", features = ["tokio", "lockfree-coordination"] }
   ```

6. **Subscribe to `SubsystemEvent` instead of polling
   `SubsystemManager::get_stats()`** when you care about sub-second
   reactivity. The polling path takes a lock per query; the event
   stream is push-based and lock-free under
   `lockfree-coordination`.

7. **`select!` on `cancelled()` in every long-running loop** — not
   just the main daemon loop. v1.1.0 fixed this for the daemon
   loop, but each subsystem still owns its own shutdown latency.
   If a subsystem polls something every N seconds without racing
   the shutdown future, expect shutdown to take up to N seconds
   for that subsystem.

### For Benchmark Fairness

- **Close background applications** before running benchmarks.
- **Disable CPU frequency scaling** in BIOS if available (use the
  `performance` governor on Linux).
- **Run multiple times** and average results to account for thermal
  throttling.
- **Use `cargo bench -- --verbose`** to see individual sample
  measurements.

<br><br><br>

<!--
:: COPYRIGHT
============================================================================ -->
<div align="center">
  <br>
  <h2></h2>
  <sup>COPYRIGHT <small>&copy;</small> 2026 <strong>JAMES GOBER.</strong></sup>
</div>
