<h1 align="center">
    <picture>
        <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/jamesgober/jamesgober/main/media/jamesgober-logo-dark.png">
        <img width="72" height="72" alt="James Gober - brand logo. Image displays stylish 'JG' initials encased in a hexagon outline." src="https://raw.githubusercontent.com/jamesgober/jamesgober/main/media/jamesgober-logo.png">
    </picture>
    <br>
    <b>CHANGELOG</b>
</h1>
<p>
  All notable changes to this project will be documented in this file. The format is based on <a href="https://keepachangelog.com/en/1.0.0/">Keep a Changelog</a>,
  and this project adheres to <a href="https://semver.org/spec/v2.0.0.html/">Semantic Versioning</a>.
</p>

## [Unreleased]

### Added

### Fixed

### Changed




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


[Unreleased]: https://github.com/jamesgober/proc-daemon/compare/v0.5.0...HEAD
[0.6.0]: https://github.com/jamesgober/proc-daemon/compare/v0.4.0...v0.6.0
[0.5.0]: https://github.com/jamesgober/proc-daemon/compare/v0.3.0...v0.5.0
[0.3.0]: https://github.com/jamesgober/proc-daemon/compare/v0.1.0...v0.3.0
[0.1.0]: https://github.com/jamesgober/proc-forge/releases/tag/v0.1.0