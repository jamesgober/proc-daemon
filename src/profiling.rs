//! Optional profiling utilities (CPU and heap) behind the `profiling` feature.
//!
//! Safe-by-default helpers to start/stop CPU profiling to a file and to capture
//! a heap profile snapshot on demand. No runtime overhead unless enabled.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::error::{Error, Result};
use pprof::protos::Message;

/// CPU profiler handle. Drop or call `stop()` to write the profile.
pub struct CpuProfiler {
    guard: pprof::ProfilerGuard<'static>,
}

impl CpuProfiler {
    /// Start CPU profiling.
    ///
    /// # Errors
    ///
    /// Returns an error if the profiler cannot be started by the underlying `pprof` crate.
    pub fn start() -> Result<Self> {
        let guard = pprof::ProfilerGuard::new(100)
            .map_err(|e| Error::runtime_with_source("failed to start CPU profiler", e))?;
        Ok(Self { guard })
    }

    /// Stop and write a profile in protobuf format compatible with `go tool pprof`.
    ///
    /// # Errors
    ///
    /// Returns an error if building the report, encoding, creating the output file,
    /// or writing the profile fails.
    pub fn stop_to_file<P: AsRef<Path>>(self, path: P) -> Result<()> {
        let report = self
            .guard
            .report()
            .build()
            .map_err(|e| Error::runtime_with_source("failed to build CPU profile report", e))?;
        let profile = report.pprof().map_err(|e| {
            Error::runtime_with_source("failed to encode CPU profile to protobuf", e)
        })?;
        let mut f = File::create(path.as_ref()).map_err(|e| {
            Error::io_with_source(
                format!("failed to create profile file: {}", path.as_ref().display()),
                e,
            )
        })?;
        // encode_to_vec available via prost::Message (re-exported as pprof::protos::Message)
        let buf = profile.encode_to_vec();
        f.write_all(&buf)
            .map_err(|e| Error::io_with_source("failed to write CPU profile", e))?;
        Ok(())
    }
}

/// Heap profiling support (optional via `heap-profiling`).
#[cfg(feature = "heap-profiling")]
pub mod heap {
    use crate::error::Result;
    use std::path::Path;

    /// Handle to an active heap profiler. Drop to finalize.
    pub struct HeapProfiler {
        _prof: dhat::Profiler,
    }

    impl HeapProfiler {
        /// Start heap profiling. If `output` is provided, sets `DHAT_OUT` accordingly.
        ///
        /// # Errors
        ///
        /// This function does not currently return errors; it returns `Ok(Self)` on success.
        pub fn start<P: AsRef<Path>>(output: Option<P>) -> Result<Self> {
            if let Some(p) = output {
                std::env::set_var("DHAT_OUT", p.as_ref());
            }
            let profiler = dhat::Profiler::new_heap();
            Ok(Self { _prof: profiler })
        }

        /// Stop profiling by consuming the handle (drop writes the profile).
        pub fn stop(self) {
            // Drop occurs here
        }
    }
}

/// Fallback when heap-profiling feature is disabled.
#[cfg(not(feature = "heap-profiling"))]
pub mod heap {
    use crate::error::{Error, ErrorCode, Result};
    use std::path::Path;

    /// Start heap profiling is unsupported without the feature.
    pub fn start<P: AsRef<Path>>(_output: Option<P>) -> Result<()> {
        Err(Error::platform_with_code(
            ErrorCode::PlatformFeatureNotAvailable,
            "heap profiling is not available in this build",
            std::env::consts::OS,
        ))
    }
}
