# TODO

## Completed
- [x] Add a small Windows test for `ToolHelp` thread count path.
- [x] Metrics collection framework integration
- [x] Memory limit enforcement (soft limit + alert)
- [x] Performance monitoring and alerting hooks (added `with_alert_handler`; expand to external sinks)
- [x] Inter-process communication primitives
- [x] Lock-free data structures for hot paths


## ADVANCED PERFORMANCE (v0.6.0 - v0.9.0): BETA
- [x] Lock-free subsystem coordination
- [x] High-resolution timing APIs
- [X] Custom scheduler hints
- [x] Memory-mapped configuration files
  - [x] Config hot-reload watcher integration (feature: `config-watch`)
  - [x] Live config snapshot via `arc-swap` maintained by watcher
  - [x] Daemon keeps watcher handle alive; builder auto-starts when `Config.hot_reload` is true
- [x] Advanced profiling (CPU) integration
- [x] Advanced profiling (Heap) support
- [x] Custom memory allocators
- [x] NUMA-aware threading
- [x] Zero-allocation configuration parsing


## POST DEVELOPMENT (After v0.9.0 Tasks)
// Minimal, release-ready checklist
- [x] Tests: unit + integration + benchmark smoke
- [x] Clippy: `cargo clippy --all-features -D warnings`
- [x] Format: `cargo fmt --all --check`
- [x] Docs: `cargo doc --all-features` builds clean
- [ ] Dependency audit: `cargo audit` (optionally `cargo deny`)
- [ ] Security/licensing sweep: secrets, licenses ok
- [ ] Performance/resource smoke: quick throughput + RSS checks
- [x] Version/docs sync: README, docs/API.md, CHANGELOG
- [ ] Tag + release
