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

## [1.1.1] - 2026-05-19

### Fixed

- **docs.rs build**: `cargo doc --all-features` fails on current nightly because the `async-std` feature pulls in `signal-hook-async-std 0.2.2` → `async-io 1.x` → `rustix 0.37.28`, which uses internal `rustc_layout_scalar_valid_range_*` attributes that the nightly compiler no longer accepts. `[package.metadata.docs.rs]` switched from `all-features = true` to an explicit feature list that excludes `async-std`. The `async-std` runtime is already marked legacy and slated for removal in v2.0.0, so its omission from the rendered docs is acceptable. Users opting into `async-std` continue to build fine on stable. Also pinned the docs.rs target to `x86_64-unknown-linux-gnu` to remove ambiguity around `windows-monitoring`.

No source changes — Cargo.toml metadata only.

## [1.1.0] - 2026-05-19

### Added

- **`Daemon::new()`** — infallible constructor returning a `DaemonBuilder` over `Config::default()`. Eliminates the `Config::new()?` → `Daemon::builder(config)` boilerplate for the common case. The existing `Daemon::with_defaults() -> Result<DaemonBuilder>` is retained for backward compatibility but now documents `Daemon::new` as the preferred entry point.
- **`impl Default for DaemonBuilder`** — equivalent to `Daemon::new()`. Enables `DaemonBuilder::default()` and Default-bound generic contexts.
- **`ShutdownCoordinator::wait_initiated()`** — async method that resolves the moment shutdown is initiated (without waiting for subsystems to mark themselves ready). Used internally by the daemon main loop; also useful in custom integration patterns that want a `tokio::select!` arm for shutdown.

### Changed

- **Performance: `parking_lot` swap in hot paths.** Replaced `std::sync::Mutex` / `std::sync::RwLock` with `parking_lot::Mutex` / `parking_lot::RwLock` in `src/subsystem.rs`, `src/resources.rs`, and `src/daemon.rs` (`RotatingFileWriter`). `parking_lot` was already a dependency for `pool.rs`, `shutdown.rs`, and `metrics.rs`; this completes the migration. Wins: 2–3× faster locking under contention, no poisoning concerns, eliminated ~24 `.lock().unwrap()` / `.read().unwrap()` / `.write().unwrap()` calls plus 17 stale `# Panics ... mutex poisoned` doc blocks that were never actually reachable with `parking_lot`.
- **Performance: shutdown latency.** The daemon main loop previously polled subsystem health and then slept `health_check_interval` (default 30s) before re-checking the shutdown flag — meaning shutdown could be delayed by up to a full interval. The loop now races the sleep against `ShutdownCoordinator::wait_initiated()` via `tokio::select!`, so shutdown completes within microseconds of being initiated regardless of the configured interval.
- **`console` dep**: 0.15 → 0.16 (semver-compatible).

### Internal

- 17 `# Panics ... mutex is poisoned` doc blocks removed from `src/subsystem.rs` — `parking_lot::Mutex` doesn't poison, so these clauses were incorrect.
- `clippy::significant_drop_tightening` resolved by explicit `drop(inner)` calls in `RotatingFileWriterGuard::write` / `flush`.
- `clippy::new_ret_no_self` allow on `Daemon::new()` with rationale (intentional builder-returning constructor, documented).
- `clippy::too_many_lines` allow on `Daemon::run` (single state-machine; splitting harms locality).

## [1.0.1] - 2026-05-18

### Fixed

- **Windows build**: `cargo build --all-features` now compiles on Windows. `pprof` is moved under `[target.'cfg(unix)'.dependencies]` because it relies on POSIX libc types (`pthread_t`, `siginfo_t`, `ucontext_t`). The `profiling` feature still exposes the CPU profiler on Unix; `heap-profiling` remains cross-platform via `dhat`.
- **Security**: `pprof` upgraded from `0.13` to `0.14`, resolving RUSTSEC-2024-0408 (unsound `std::slice::from_raw_parts` usage). The 1.0.0 CHANGELOG claimed this was done in 1.0.0-RC2; it was not — fixed here.
- **Clippy on Rust 1.95**: cleared all 8 stable-toolchain warnings:
  - `map_unwrap_or` in `src/config.rs` and `src/daemon.rs`
  - `duration_suboptimal_units` (e.g. `Duration::from_millis(5000)` → `Duration::from_secs(5)`) in `src/config.rs` and `src/subsystem.rs`
- **Windows monitoring lints** (`windows-monitoring` feature): replaced `&mut local` with `addr_of_mut!`, switched `as` casts to `From`/`TryFrom`, inlined `format!` args, added missing `# Errors` docs in `src/ipc.rs`, and corrected the inner/outer attribute ordering on the Windows IPC module.
- **README accuracy**: removed broken links to `./dev/release-notes/v1.0.0.md` and `CONTRIBUTING.md` (neither exists in-tree), replaced the non-existent `cargo unsafe-all-targets` invocation with `cargo geiger`, and corrected the test-count claim.
- **CI workflow**:
  - `MSRV Check`: pin `indexmap` to `2.10.0` after `cargo generate-lockfile` because `indexmap 2.14.0+` requires the `edition2024` Cargo feature (stabilized in Rust 1.85), which Cargo 1.82 cannot parse. The MSRV-aware resolver landed in Cargo 1.84; this pin is a temporary backstop until MSRV is raised.
  - `Security Audit`: fixed a malformed step — the `Security audit` step was incorrectly using `actions/checkout@v4` instead of installing the toolchain; corrected to install Rust before invoking `cargo audit`.

### Changed

- **Crate metadata** (crates.io visibility):
  - Description rewritten to lead with "async daemon framework" and mention Tokio explicitly.
  - Keywords: `systemd` (misleading — no systemd integration) replaced with `async`.
  - Categories: `network-programming` (incorrect — no networking in this crate) and `development-tools` (too generic) replaced with `asynchronous` and `command-line-utilities`.
- **Dependency bumps** (semver-compatible):
  - `tokio` 1.37 → 1.52
  - `parking_lot` 0.12 → 0.12.5
  - `arc-swap` 1.7 → 1.9
  - `dashmap` 6.0 → 6.2
  - `once_cell` 1.19 → 1.21
  - `fastrand` 2.0 → 2.4
  - `pprof` 0.13 → 0.14
  - `proptest` (dev) 1.6 → 1.11 (also resolves rand-tree warnings)
- **`.cargo/audit.toml`**: added rationale comments for each allowlist entry. `RUSTSEC-2025-0052` (async-std discontinued) and `RUSTSEC-2024-0384` (instant unmaintained) remain allow-listed for the optional `async-std` feature path (to be removed in v2.0.0). Added `RUSTSEC-2026-0097` (dev-only rand soundness via proptest 1.11).

### Removed

- `dhat` dep declaration no longer hidden behind a quoted bare key (`"dhat"` → `dhat`).

## [1.0.0] - 2026-02-23

### Added

- Stable release: ready for production deployments
- Audit allowlist for RustSec warnings on discontinued `async-std` and `instant`

### Changed

- Version: bumped crate version to `1.0.0` (stable release)
- Dependencies: upgraded `bytes` to 1.11.1, `pprof` to 0.14.1, `tokio` to 1.49.0, `tokio-test` to 0.4.5
- Docs: mark `async-std` support as best-effort legacy; mark pprof as Unix-preferred

### Fixed

- Security: resolved RUSTSEC-2026-0007 (bytes integer overflow in `BytesMut::reserve`)
- Safety: documented RUSTSEC-2024-0408 (pprof unsound but optional)
- Code quality: fixed all 6 clippy violations (docs, imports, drop timing, casts, format strings)

## [1.0.0-RC2] - 2026-02-23

### Added

- Audit allowlist for RustSec warnings on discontinued `async-std` and `instant`

### Changed

- Version: bumped crate version to `1.0.0-rc2`
- Dependencies: upgraded `bytes` to 1.11.1, `pprof` to 0.14.1, `tokio` to 1.49.0, `tokio-test` to 0.4.5
- Docs: mark `async-std` support as best-effort legacy and update installation snippets to `1.0.0-rc2`

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


[Unreleased]: https://github.com/jamesgober/proc-daemon/compare/v1.1.1...HEAD
[1.1.1]: https://github.com/jamesgober/proc-daemon/compare/v1.1.0...v1.1.1
[1.1.0]: https://github.com/jamesgober/proc-daemon/compare/v1.0.1...v1.1.0
[1.0.1]: https://github.com/jamesgober/proc-daemon/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/jamesgober/proc-daemon/compare/v1.0.0-rc2...v1.0.0
[1.0.0-RC2]: https://github.com/jamesgober/proc-daemon/compare/v1.0.0-rc.1...v1.0.0-rc2
[1.0.0-RC.1]: https://github.com/jamesgober/proc-daemon/compare/v0.9.0...v1.0.0-rc.1
[0.9.0]: https://github.com/jamesgober/proc-daemon/compare/v0.6.1...v0.9.0
[0.6.1]: https://github.com/jamesgober/proc-daemon/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/jamesgober/proc-daemon/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/jamesgober/proc-daemon/compare/v0.3.0...v0.5.0
[0.3.0]: https://github.com/jamesgober/proc-daemon/compare/v0.1.0...v0.3.0
[0.1.0]: https://github.com/jamesgober/proc-daemon/releases/tag/v0.1.0
