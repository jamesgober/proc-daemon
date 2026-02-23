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

Typical performance characteristics (Criterion, release, v1.0.0-rc2):
- **Daemon Creation**: ~1.87μs per init
- **Subsystem Registration** (3 tasks): ~3.37μs per build
- **Config Build**: ~92ns per build
- **Shutdown Coordination** (single quick task): ~2.56μs per cycle
- **Error Creation**: ~22.8ns per error
- **Error Chain Construction**: ~56.7ns per chain

Benchmarks unchanged from the previous release; results carried forward.

### Memory Usage

- **Base daemon**: ~1-2MB
- **Per subsystem**: ~4-8KB
- **Configuration**: ~1-4KB
- **Signal handling**: ~512B

<!-- PERFORMANCE DATA -->
## Core Performance Metrics

| Metric | Result (median) | Comparison | Improvement |
|--------|------------------|------------|-------------|
| Daemon creation | 1.87 μs | 9.0 μs (v0.9.0) | ≈79% faster |
| Subsystem registration (3 tasks) | 3.37 μs | 12.9 μs (v0.9.0) | ≈74% faster |
| Config build | 92.1 ns | 234 ns (v0.9.0) | ≈61% faster |
| Shutdown coordination (1 task) | 2.56 μs | 10.2 μs (v0.9.0) | ≈75% faster |
| Error creation | 22.8 ns | 71.7 ns (v0.9.0) | ≈68% faster |
| Error cause chain | 56.7 ns | 539 ns (v0.9.0) | ≈90% faster |


<br>

## Version Performance History

Results generated with Criterion, default configuration, Tokio runtime, no metrics feature (unless noted).

| Version | Date | Daemon create | Subsystem reg (3) | Config build | Shutdown (1 task) | Error create | Error chain |
|---------|------|---------------|-------------------|--------------|-------------------|--------------|-------------|
| 1.0.0-rc2 | 2026-02-23 | 1.87 μs | 3.37 μs | 92.1 ns | 2.56 μs | 22.8 ns | 56.7 ns |
| 0.9.0 | 2025-08-26 | 9.0 μs | 12.9 μs | 234 ns | 10.2 μs | 71.7 ns | 539 ns |



<br><br><br>

<!--
:: COPYRIGHT
============================================================================ -->
<div align="center">
  <br>
  <h2></h2>
  <sup>COPYRIGHT <small>&copy;</small> 2025 <strong>JAMES GOBER.</strong></sup>
</div>