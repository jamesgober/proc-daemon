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
  - Integrated filesystem watcher to monitor config file and live-reload on changes.
  - Added live config snapshot maintained via `arc-swap::ArcSwap` and exposed via `Daemon::config_snapshot()` (feature-gated).
  - `DaemonBuilder::build()` auto-starts the watcher when `Config.hot_reload` is true and keeps the watcher handle alive.

- Subsystem events: `SubsystemManager::enable_events()` and `try_next_event()` publish non-blocking `SubsystemEvent::StateChanged` notifications for state transitions.
- API: `SubsystemManager::subscribe_events()` (behind `lockfree-coordination`) to obtain a cloned receiver for subsystem events.

### Fixed

- Error conversions: map `notify::Error` into project `Error` using `runtime_with_source` in `src/config.rs` to fix compilation with `config-watch` + `toml` + `mmap-config`.
- Clippy/lints cleanup across core modules:
  - `src/config.rs`: removed unnecessary `let _ =` around match in watcher callback (`let_unit_value`).
  - `src/coord.rs`: added `# Errors` docs to `chan::try_recv()` and backticked `std::sync::mpsc` (`doc_markdown`, `missing_errors_doc`).
  - `src/subsystem.rs`: tightened lock lifetimes and added `# Panics` docs (`significant_drop_tightening`, `missing_panics_doc`); simplified `try_next_event()` to use `.ok()` idiom.
  - `src/profiling.rs`: added `# Errors` docs, replaced wildcard imports in heap module, backticked `DHAT_OUT`, and gated `ErrorCode` import correctly.
- Clippy cleanup (examples + library):
  - `src/coord.rs`: add `#[must_use]` to `chan::unbounded()`.
  - `src/resources.rs`: avoid repeated `Instant::now()` comparisons; round CPU milli-percent before casting; prefer `u64::from` for thread count.
  - Verified clean with `cargo clippy -- -D warnings`, `--all-features`, and `--examples`.

### Changed

- Docs: Updated `README.md` feature matrix and added usage sections for `mimalloc` and `high-res-timing`.
- Docs: Updated `docs/API.md` installation snippet versions and added Feature Flags section mirroring `README.md`.
- Internal: Feature-gated global allocator hookup when `mimalloc` is enabled; added `timing::now()` returning `quanta::Instant` when `high-res-timing` is enabled.
- Internal: When `scheduler-hints` is enabled, hooks are invoked at daemon startup (`Daemon::run()`). Behavior remains no-op unless `scheduler-hints-unix` is also enabled on Unix, where a best-effort `renice` is attempted.
- Internal: Introduced coordination facade `proc_daemon::coord::chan` with a uniform API across backends.
- Docs/Examples: Added event subscription coverage (`subscribe_events()`) and event polling worker wiring in `examples/simple.rs` for Tokio and async-std.
 - Internal: Integrated optional config watcher into `src/daemon.rs` guarded by `config-watch`; preserved `config()` and added `config_snapshot()` (feature-gated).
 - Internal: Added best-effort CPU affinity application on Linux when `scheduler-hints-unix` is enabled (uses `nix::sched::sched_setaffinity`). No-ops on other platforms.
 - Performance: Reworked TOML config fast-path to avoid intermediate allocations and unsafe mmap, honoring `#![deny(unsafe_code)]`. Still falls back to Figment providers on error.



## [0.6.1] - 2025-08-21

### Changed

- Crates.io metadata only: updated `Cargo.toml` keywords/categories to meet 5-item limits.
- Housekeeping: moved developer TODOs to `dev/TODO.md`.
- No functional changes. No public API changes.


## [0.6.0] - 2025-08-21

### Fixed

- Resolved Clippy warnings:
  - `duplicated_attributes`: removed crate-level cfg in `src/ipc.rs` (already gated in `src/lib.rs`).
  - `missing_errors_doc`: added `# Errors` sections to `unix::bind()` and `unix::connect()` in `src/ipc.rs`.
  - `format_push_string`, `write_with_newline`: switched to `write!/writeln!` in `src/metrics.rs` to avoid extra allocations and newline lint.
  - `redundant_closure_for_method_calls`: used `std::time::Duration::as_secs_f64` method reference in `src/metrics.rs`.
  - `uninlined_format_args`: inlined format args for `{k}_count` and `{k}_sum` in `src/metrics.rs`.

### Changed

- Refactored Prometheus rendering in `src/metrics.rs` for lower allocations and clearer formatting.
- Ran `cargo fmt` to standardize formatting across the codebase.




## [0.5.0] - 2025-08-20
### Added
- Windows process monitoring support using Win32 ToolHelp thread enumeration (no WDK dependency required)

### Fixed
- Resolved Windows build errors by enabling required `windows` crate features: `Win32_System_Diagnostics(_ToolHelp)`, `Win32_System_Threading`, `Win32_System_ProcessStatus`, `Win32_Foundation`
- Eliminated clippy pedantic warnings in `src/resources.rs`:
  - similar name bindings (renamed child CPU time vars)
  - potential truncation/precision cast warnings (scoped allows and safer conversions)
- Removed unused `ErrorCode` import in `src/signal.rs` and qualified enum usage to avoid platform-gated unused import
- Fixed benchmark config panic by ensuring `force_shutdown_timeout > shutdown_timeout`
- Deduplicated and reordered imports to satisfy rustfmt across platforms

### Changed
- Replaced WDK `NtQuerySystemInformation` usage with Win32 ToolHelp APIs for thread counting on Windows
- Tightened runtime gating in `src/signal.rs` for no-runtime builds (clear `MissingRuntime` error)



## [0.4.0] - 2025-08-20

### Fixed
- Fixed `trivially_copy_pass_by_ref` warnings in `error.rs` by modifying `ErrorCode::as_str` to take `self` by value
- Fixed `needless_pass_by_value` warnings in `shutdown.rs` and `subsystem.rs` by updating methods to take string slices
- Removed unused imports in `daemon.rs`
- Fixed `redundant_else` block in `daemon.rs` for logging configuration
- Fixed type mismatch in `shutdown.rs` where `String` was passed to method expecting `&str`

### Changed
- Several clippy pedantic warnings remain as documented issues for future improvements:
  - Missing error documentation in various functions returning `Result`
  - Potential precision loss in numeric casts (u128 to u64, usize/u64 to f32/f64)
  - Structure with excessive boolean fields in `SignalConfig`
  - Unused `async` function that contains no `await` statements




## [0.3.0] - 2025-08-20
### Added
- Implemented object pooling system for efficient memory reuse
- Added string pooling to reduce allocations in hot paths
- Added vector pooling for common container types
- Implemented shutdown coordination mechanisms with timeout support
- Added subsystem lifecycle management with state tracking
- Added health monitoring hooks for process status reporting
- Created comprehensive signal handling across platforms
- Added custom signal handler registration capabilities
- Zero-allocation hot paths for critical operations
- Pre-allocated collections in performance-sensitive areas
- Created configurable restart policies for subsystems
- Implemented cross-platform file locking to prevent multiple daemon instances
- Added resource usage tracking (memory, CPU, thread count) with history support
- Created platform-specific implementations for Linux, macOS, and Windows resource monitoring

### Fixed
- Added timeout wrappers around all async tests to prevent freezing
- Improved subsystem shutdown handling to avoid deadlocks and timeouts
- Modified test_shutdown_coordination to properly handle async notifications
- Added timeouts to stop_subsystem method to prevent hanging on task completion
- Fixed config_builder test to ensure proper timeout validation
- Increased test robustness with explicit timeouts on critical operations
- Fixed duplicate implementation in ObjectPool to remove code redundancy
- Fixed object pool test to properly validate pooled object behavior
- Removed unused imports in subsystem module
- Fixed unnecessary mutable variable warnings
- Properly handled unused return values in shutdown module
- Fixed duplicate error code values in ErrorCode enum
- Fixed incorrect error constructor references in file locking code
- Fixed unlocking name collision using fully qualified path
- Fixed doctest failures in error handling module
- Fixed clippy warnings in resource tracking implementation
- Added missing error documentation for public functions

### Changed
- Enhanced error handling with thiserror integration
- Improved subsystem registration with memory pooling
- Optimized signal handler registration process
- Implemented more robust shutdown sequence with timeouts


## [0.1.0] - 2025-08-19

Initial pre-dev release for backup.

### Added
- `Cargo.toml` file.
- `docs/API` file.
- `docs/PRINCIPLES` file.
- `docs/PERFORMANCE` file.
- `docs/README` file.
- `CHANGELOG` file.
- `LICENSE` file.
- `README` file.


[Unreleased]: https://github.com/jamesgober/proc-daemon/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/jamesgober/proc-daemon/compare/v0.6.1...v0.9.0
[0.6.1]: https://github.com/jamesgober/proc-daemon/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/jamesgober/proc-daemon/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/jamesgober/proc-daemon/compare/v0.3.0...v0.5.0
[0.3.0]: https://github.com/jamesgober/proc-daemon/compare/v0.1.0...v0.3.0
[0.1.0]: https://github.com/jamesgober/proc-forge/releases/tag/v0.1.0