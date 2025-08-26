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

Typical performance characteristics (Criterion, release, v0.9.0):
- **Daemon Creation**: ~9.0μs per init
- **Subsystem Registration** (3 tasks): ~12.9μs per build
- **Config Build**: ~234ns per build
- **Shutdown Coordination** (single quick task): ~10.2μs per cycle
- **Error Creation**: ~71.7ns per error
- **Error Chain Construction**: ~539ns per chain

### Memory Usage

- **Base daemon**: ~1-2MB
- **Per subsystem**: ~4-8KB
- **Configuration**: ~1-4KB
- **Signal handling**: ~512B

<!-- PERFORMANCE DATA -->
## Core Performance Metrics

| Metric | Result (median) | Comparison | Improvement |
|--------|------------------|------------|-------------|
| Daemon creation | 9.0 μs | — | — |
| Subsystem registration (3 tasks) | 12.9 μs | — | — |
| Config build | 234 ns | — | — |
| Shutdown coordination (1 task) | 10.2 μs | — | — |
| Error creation | 71.7 ns | — | — |
| Error cause chain | 539 ns | — | — |


<br>

## Version Performance History

Results generated with Criterion, default configuration, Tokio runtime, no metrics feature (unless noted).

| Version | Date | Daemon create | Subsystem reg (3) | Config build | Shutdown (1 task) | Error create | Error chain |
|---------|------|---------------|-------------------|--------------|-------------------|--------------|-------------|
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