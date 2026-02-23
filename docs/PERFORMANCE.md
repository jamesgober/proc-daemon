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

Typical performance characteristics (Criterion, release, v1.0.0):
- **Daemon Creation**: ~2.14μs per init
- **Subsystem Registration** (3 tasks): ~3.76μs per build
- **Config Build**: ~80.6ns per build
- **Shutdown Coordination** (single quick task): ~2.87μs per cycle
- **Error Creation**: ~31.2ns per error
- **Error Chain Construction**: ~137ns per chain

Note: Benchmarks measured on 2026-02-23; slight performance variations (±5-10%) may occur due to system load and thermal conditions. All metrics remain within acceptable production bounds.

## Benchmark Methodology

All benchmarks are generated using [Criterion.rs](https://bheisler.github.io/criterion.rs/book/):

**Test Configuration:**
- **Compiler**: Rust 1.82.0+, release mode with debuginfo
- **Runtime**: Tokio (async runtime)
- **Features**: tokio (core) only; no metrics collection
- **Platform**: Windows x86_64 (benchmarks may vary on Linux/macOS due to syscall overhead)
- **Warmup**: 3 seconds per benchmark
- **Samples**: 100 measurements per benchmark
- **Iterations**: Auto-scaled by Criterion to achieve stable measurements

**Interpretation Guidance:**
- Results are median values; see [Version Performance History](#version-performance-history) for full timing ranges
- ±5-10% variance is normal between runs on the same machine
- Larger variance may occur with system thermal throttling or background load
- Actual latency in production may differ based on workload complexity and subsystem count

### Memory Usage

- **Base daemon**: ~1-2MB
- **Per subsystem**: ~4-8KB
- **Configuration**: ~1-4KB
- **Signal handling**: ~512B

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

## Version Performance History

Results generated with Criterion, default configuration, Tokio runtime, no metrics feature (unless noted).

| Version | Date | Daemon create | Subsystem reg (3) | Config build | Shutdown (1 task) | Error create | Error chain |
|---------|------|---------------|-------------------|--------------|-------------------|--------------|-------------|
| 1.0.0 | 2026-02-23 | 2.14 μs | 3.76 μs | 80.6 ns | 2.87 μs | 31.2 ns | 137 ns |
| 0.9.0 | 2025-08-26 | 9.0 μs | 12.9 μs | 234 ns | 10.2 μs | 71.7 ns | 539 ns |

## Running Benchmarks Yourself

To run the benchmark suite on your own hardware:

```bash
# Run all benchmarks with Tokio
cargo bench --bench daemon_bench --features tokio

# Run benchmarks with metrics enabled (will be slightly slower)
cargo bench --bench daemon_bench --features "tokio metrics"

# Compare against previous runs (requires --verbose)
cargo bench --bench daemon_bench --features tokio -- --verbose
```

**Note on Windows:** Some syscall-related benchmarks may show higher latencies than Linux due to Windows kernel architecture differences. This is expected and doesn't indicate a problem.

## Performance Optimization Tips

### For Your Application

1. **Enable `mimalloc` in production** if your daemon allocates heavily:
   ```toml
   proc-daemon = { version = "1.0.0", features = ["tokio", "mimalloc"] }
   ```

2. **Use `high-res-timing` for sub-microsecond operations**:
   ```toml
   proc-daemon = { version = "1.0.0", features = ["tokio", "high-res-timing"] }
   ```

3. **Profile your subsystems** using `--features profiling` (Unix only) to identify hot paths

4. **Enable scheduler hints on Unix** for reduced context switching:
   ```toml
   proc-daemon = { version = "1.0.0", features = ["tokio", "scheduler-hints-unix"] }
   ```

5. **Use lock-free coordination** for high-frequency inter-subsystem messaging:
   ```toml
   proc-daemon = { version = "1.0.0", features = ["tokio", "lockfree-coordination"] }
   ```

### For Benchmark Fairness

- **Close background applications** before running benchmarks
- **Disable CPU frequency scaling** in BIOS if available (use `performance` governor on Linux)
- **Run multiple times** and average results to account for thermal throttling
- **Use `cargo bench -- --verbose`** to see individual sample measurements



<br><br><br>

<!--
:: COPYRIGHT
============================================================================ -->
<div align="center">
  <br>
  <h2></h2>
  <sup>COPYRIGHT <small>&copy;</small> 2026 <strong>JAMES GOBER.</strong></sup>
</div>