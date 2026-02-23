<h1 align="center">
    <img width="72xpx" src="./media/proc-rs-orange.svg" alt="High-Performance Process EcoSystem for Rust">
    <br>
    <b>CHANGELOG</b>
</h1>
<p>
  All notable changes to this project will be documented in this file. The format is based on <a href="https://keepachangelog.com/en/1.0.0/">Keep a Changelog</a>,
  and this project adheres to <a href="https://semver.org/spec/v2.0.0.html/">Semantic Versioning</a>.
</p>

## [Unreleased]

- _No changes yet._

## [1.0.0-RC.2] - 2026-02-23

### Added

- Audit allowlist for RustSec warnings on discontinued `async-std` and `instant`

### Changed

- Version: bumped crate version to `1.0.0-rc.2`
- Dependencies: upgraded `bytes` to 1.11.1, `pprof` to 0.14.1, `tokio` to 1.49.0, `tokio-test` to 0.4.5
- Docs: mark `async-std` support as best-effort legacy and update installation snippets to `1.0.0-rc.2`

### Fixed

- Security: resolved RUSTSEC-2026-0007 (bytes integer overflow in `BytesMut::reserve`)
- Safety: addressed RUSTSEC-2024-0408 by upgrading `pprof`

## [1.0.0-RC.1] - 2026-01-30

### Performance

- Shutdown polling uses exponential backoff (1ms → 50ms) instead of fixed 50ms — 77% faster shutdown coordination
- Metrics collection fast-path uses read-only locks; write locks only for new metrics — eliminates contention
- Replaced `std::sync::Mutex` with `parking_lot::Mutex` in object pools — 2-3x faster under contention
- Optimized atomic orderings: `Relaxed` for single-writer hot paths (shutdown flags, readiness checks)
- Batched metadata updates in subsystem tasks: single lock acquisition instead of 2-3
- Eliminated Vec allocations in daemon health check loop with early-exit pattern
- Pre-sized result vectors in stats collection to avoid reallocations
- Moved config validation from `run()` to `build()` — fail-fast semantics

**Benchmark Results:**
- Daemon creation: 1.64µs (was 8.4µs)
- Subsystem registration: 2.82µs (was 13µs)
- Config loading: 92.5ns (was 232ns)
- Shutdown coordination: 2.33µs (was 10.2µs)
- Error creation: 22ns (was 74ns)
- Metrics operations: 41.4ns counters, 36.9ns gauges

### Added

- Logging: size-based rotation for file logging with `LogConfig.max_file_size` and `LogConfig.max_files`
- Shutdown: explicit kill timeout handling via `ShutdownCoordinator::wait_for_kill_shutdown()`

### Changed

- Version: bumped crate version to `1.0.0-rc.1`
- MSRV: bumped from 1.75.0 to 1.82.0 to resolve `indexmap` dependency compatibility
- Dependencies: updated `tracing-subscriber` to 0.3.20 to fix RUSTSEC-2025-0055
- Shutdown coordination now tracks graceful, force, and kill timeouts independently
- Config hot-reload watches the configured path (or `work_dir` + `DEFAULT_CONFIG_FILE` when provided)
- Metrics histograms capped to prevent unbounded growth
- Resource history uses a ring buffer to avoid O(n) trimming

### Fixed

- Fixed unsafe code in Linux clock tick retrieval (added safety documentation)
- Fixed all clippy warnings and compilation errors across CI/CD platforms
- Fixed unused import warnings in config, signal, and resources modules
- Replaced manual `Default` implementations with `#[derive(Default)]` per clippy
- Fixed unchecked time subtraction using `checked_sub` in shutdown module
- Replaced `once_cell::sync::Lazy` with `std::sync::LazyLock` for high-res timing
- Streamlined health check logic using `is_none_or` instead of `map_or`
- `ShutdownHandle::cancelled()` now short-circuits when shutdown already initiated on Tokio
- `SubsystemManager::stop_subsystem()` only reports readiness after task completion
- Async-std Unix signal handling now registers SIGTERM/SIGINT/SIGQUIT/SIGHUP correctly
- Linux `/proc/[pid]/stat` parsing handles space-containing process names
- macOS sampling uses absolute `/bin/ps` path to avoid PATH injection
- IPC Unix socket binding no longer removes non-socket paths or symlinks
- `Config::new()` validates defaults; `work_dir` is validated at startup
- Daemon main loop avoids busy spinning when no async runtime is enabled
- Removed unused `shutdown_fix.rs` module
- Updated `src/ipc.rs` Windows module documentation


## [0.9.0] - 2025-08-26

### Added

- Optional feature flags:
  - `mimalloc`: opt-in global allocator for allocation-heavy workloads
  - `high-res-timing`: exposes `proc_daemon::timing` backed by `quanta` for fast monotonic timestamps
  - `scheduler-hints`: exposes `proc_daemon::scheduler` hooks (no-op by default)
  - `scheduler-hints-unix`: best-effort Unix niceness adjustment via `renice` (no-op without privileges)
  - `lockfree-coordination`: enables lock-free MPMC event channel via `crossbeam-channel` in `proc_daemon::coord`
  - `profiling`: optional CPU profiling helpers in `proc_daemon::profiling` using `pprof`
  - `heap-profiling`: optional heap profiling via `dhat` in `proc_daemon::profiling::heap`
  - `mmap-config`: reserved flag for config fast-path (kept for compatibility)

- Configuration hot-reload (feature: `config-watch`):
  - Integrated filesystem watcher to monitor config file and live-reload on changes
  - Added live config snapshot via `arc-swap::ArcSwap` exposed through `Daemon::config_snapshot()` (feature-gated)
  - `DaemonBuilder::build()` auto-starts the watcher when `Config.hot_reload` is true

- Subsystem events: `SubsystemManager::enable_events()` and `try_next_event()` publish non-blocking `SubsystemEvent::StateChanged` notifications
- API: `SubsystemManager::subscribe_events()` (behind `lockfree-coordination`) to obtain a cloned receiver for subsystem events

### Fixed

- Error conversions: map `notify::Error` into project `Error` using `runtime_with_source` in `src/config.rs`
- Clippy/lints cleanup across core modules:
  - `src/config.rs`: removed unnecessary `let _ =` around match in watcher callback
  - `src/coord.rs`: added `# Errors` docs to `chan::try_recv()` and backticked `std::sync::mpsc`
  - `src/subsystem.rs`: tightened lock lifetimes and added `# Panics` docs; used `.ok()` idiom in `try_next_event()`
  - `src/profiling.rs`: added `# Errors` docs, replaced wildcard imports in heap module
- Clippy cleanup (examples + library):
  - `src/coord.rs`: add `#[must_use]` to `chan::unbounded()`
  - `src/resources.rs`: avoid repeated `Instant::now()` comparisons; round CPU milli-percent before casting
  - Clean across `cargo clippy -- -D warnings`, `--all-features`, and `--examples`

### Changed

- Docs: Updated `README.md` feature matrix and added usage sections for `mimalloc` and `high-res-timing`
- Docs: Updated `docs/API.md` installation snippet versions and added Feature Flags section
- Internal: Feature-gated global allocator hookup when `mimalloc` is enabled
- Internal: When `scheduler-hints` is enabled, hooks are invoked at daemon startup
- Internal: Introduced coordination facade `proc_daemon::coord::chan` with a uniform API across backends
- Internal: Integrated optional config watcher into `src/daemon.rs` guarded by `config-watch`
- Internal: Added best-effort CPU affinity application on Linux when `scheduler-hints-unix` is enabled
- Performance: Reworked TOML config fast-path to avoid intermediate allocations and unsafe mmap


## [0.6.1] - 2025-08-21

### Changed

- Crates.io metadata only: updated `Cargo.toml` keywords/categories to meet 5-item limits
- Housekeeping: moved developer TODOs to `dev/TODO.md`
- No functional changes. No public API changes.


## [0.6.0] - 2025-08-21

### Fixed

- Resolved Clippy warnings:
  - `duplicated_attributes`: removed crate-level cfg in `src/ipc.rs` (already gated in `src/lib.rs`)
  - `missing_errors_doc`: added `# Errors` sections to `unix::bind()` and `unix::connect()`
  - `format_push_string`, `write_with_newline`: switched to `write!/writeln!` in `src/metrics.rs`
  - `redundant_closure_for_method_calls`: used method reference in `src/metrics.rs`
  - `uninlined_format_args`: inlined format args in `src/metrics.rs`

### Changed

- Refactored Prometheus rendering in `src/metrics.rs` for lower allocations
- Ran `cargo fmt` to standardize formatting


## [0.5.0] - 2025-08-20

### Added
- Windows process monitoring support using Win32 ToolHelp thread enumeration (no WDK dependency)

### Fixed
- Resolved Windows build errors by enabling required `windows` crate features
- Eliminated clippy pedantic warnings in `src/resources.rs`
- Removed unused `ErrorCode` import in `src/signal.rs`
- Fixed benchmark config panic by ensuring `force_shutdown_timeout > shutdown_timeout`
- Deduplicated and reordered imports to satisfy rustfmt across platforms

### Changed
- Replaced WDK `NtQuerySystemInformation` with Win32 ToolHelp APIs for thread counting on Windows
- Tightened runtime gating in `src/signal.rs` for no-runtime builds


## [0.4.0] - 2025-08-20

### Fixed
- Fixed `trivially_copy_pass_by_ref` warnings in `error.rs`
- Fixed `needless_pass_by_value` warnings in `shutdown.rs` and `subsystem.rs`
- Removed unused imports in `daemon.rs`
- Fixed `redundant_else` block in `daemon.rs`
- Fixed type mismatch in `shutdown.rs`


## [0.3.0] - 2025-08-20

### Added
- Object pooling system for efficient memory reuse (string and vector pools)
- Shutdown coordination with timeout support
- Subsystem lifecycle management with state tracking and restart policies
- Health monitoring hooks for process status reporting
- Comprehensive cross-platform signal handling with custom handler registration
- Cross-platform file locking to prevent multiple daemon instances
- Resource usage tracking (memory, CPU, thread count) with history support
- Platform-specific implementations for Linux, macOS, and Windows

### Fixed
- Added timeout wrappers around all async tests to prevent freezing
- Improved subsystem shutdown handling to avoid deadlocks
- Fixed duplicate implementation in ObjectPool
- Fixed duplicate error code values in ErrorCode enum
- Fixed incorrect error constructor references in file locking code
- Fixed doctest failures in error handling module

### Changed
- Enhanced error handling with thiserror integration
- Optimized signal handler registration


## [0.1.0] - 2025-08-19

Initial pre-dev release.

### Added
- Project scaffolding, documentation structure, and license


[Unreleased]: https://github.com/jamesgober/proc-daemon/compare/v1.0.0-rc.2...HEAD
[1.0.0-RC.2]: https://github.com/jamesgober/proc-daemon/compare/v1.0.0-rc.1...v1.0.0-rc.2
[1.0.0-RC.1]: https://github.com/jamesgober/proc-daemon/compare/v0.9.0...v1.0.0-rc.1
[0.9.0]: https://github.com/jamesgober/proc-daemon/compare/v0.6.1...v0.9.0
[0.6.1]: https://github.com/jamesgober/proc-daemon/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/jamesgober/proc-daemon/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/jamesgober/proc-daemon/compare/v0.3.0...v0.5.0
[0.3.0]: https://github.com/jamesgober/proc-daemon/compare/v0.1.0...v0.3.0
[0.1.0]: https://github.com/jamesgober/proc-daemon/releases/tag/v0.1.0
