// Copyright 2023 James Gober. All rights reserved.
// Use of this source code is governed by Apache License
// that can be found in the LICENSE file.

//! # Resource Usage Tracking
//!
//! This module provides functionality for tracking process resource usage,
//! including memory and CPU utilization.
//!
//! ## Features
//!
//! - Cross-platform memory usage tracking
//! - CPU usage monitoring with percentage calculations
//! - Sampling at configurable intervals
//! - Historical data collection with time-series support
//!
//! ## Example
//!
//! ```ignore
//! use proc_daemon::resources::{ResourceTracker, ResourceUsage};
//! use std::time::Duration;
//!
//! // Create a new resource tracker sampling every second
//! let mut tracker = ResourceTracker::new(Duration::from_secs(1));
//!
//! // Get the current resource usage
//! let usage = tracker.current_usage();
//! println!("Memory: {}MB, CPU: {}%", usage.memory_mb(), usage.cpu_percent());
//! ```
//!
//! With tokio runtime:
//!
//! ```ignore
//! # use proc_daemon::resources::ResourceTracker;
//! # use std::time::Duration;
//! # let mut tracker = ResourceTracker::new(Duration::from_secs(1));
//! #[cfg(feature = "tokio")]
//! async {
//!     // Start tracking
//!     tracker.start().unwrap();
//!     
//!     // ... use the tracker ...
//!     
//!     // Stop tracking when done
//!     tracker.stop().await;
//! };
//! ```
//!
//! With async-std runtime:
//!
//! ```ignore
//! # use proc_daemon::resources::ResourceTracker;
//! # use std::time::Duration;
//! # let mut tracker = ResourceTracker::new(Duration::from_secs(1));
//! #[cfg(all(feature = "async-std", not(feature = "tokio")))]
//! async {
//!     // Start tracking
//!     tracker.start().unwrap();
//!     
//!     // ... use the tracker ...
//!     
//!     // Stop tracking when done
//!     tracker.stop();
//! };
//! ```

#[allow(unused_imports)]
use crate::error::{Error, Result};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

// Runtime-specific JoinHandle types
#[cfg(all(feature = "async-std", not(feature = "tokio")))]
use async_std::task::JoinHandle as AsyncJoinHandle;
#[cfg(not(any(feature = "tokio", feature = "async-std")))]
use std::thread::JoinHandle;
#[cfg(feature = "tokio")]
#[allow(unused_imports)]
use tokio::task::JoinHandle;
#[cfg(feature = "tokio")]
#[allow(unused_imports)]
use tokio::time;

// OS-specific imports
#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(target_os = "linux")]
use std::io::{BufRead, BufReader};

#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(all(target_os = "windows", feature = "windows-monitoring"))]
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
};
#[cfg(all(target_os = "windows", feature = "windows-monitoring"))]
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
#[cfg(all(target_os = "windows", feature = "windows-monitoring"))]
use windows::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_INFORMATION};

#[cfg(all(target_os = "windows", feature = "windows-monitoring"))]
use windows::Win32::Foundation::{CloseHandle, FILETIME};

/// Represents the current resource usage of the process
#[derive(Debug, Clone)]
pub struct ResourceUsage {
    /// Timestamp when the usage was recorded
    timestamp: Instant,

    /// Memory usage in bytes
    memory_bytes: u64,

    /// CPU usage as a percentage (0-100)
    cpu_percent: f64,

    /// Number of threads in the process
    thread_count: u32,
}

impl ResourceUsage {
    /// Creates a new `ResourceUsage` with the current time
    #[must_use]
    pub fn new(memory_bytes: u64, cpu_percent: f64, thread_count: u32) -> Self {
        Self {
            timestamp: Instant::now(),
            memory_bytes,
            cpu_percent,
            thread_count,
        }
    }

    /// Returns the memory usage in bytes
    #[must_use]
    pub const fn memory_bytes(&self) -> u64 {
        self.memory_bytes
    }

    /// Returns the memory usage in megabytes
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn memory_mb(&self) -> f64 {
        // Simplify calculation for better accuracy
        self.memory_bytes as f64 / 1_048_576.0
    }

    /// Returns the CPU usage as a percentage (0-100)
    #[must_use]
    pub const fn cpu_percent(&self) -> f64 {
        self.cpu_percent
    }

    /// Returns the number of threads in the process
    #[must_use]
    pub const fn thread_count(&self) -> u32 {
        self.thread_count
    }

    /// Returns the time elapsed since this usage was recorded
    #[must_use]
    pub fn age(&self) -> Duration {
        self.timestamp.elapsed()
    }
}

/// Provides resource tracking functionality for the current process
#[allow(dead_code)]
pub struct ResourceTracker {
    /// The interval at which to sample resource usage
    sample_interval: Duration,

    /// The current resource usage
    current_usage: Arc<RwLock<ResourceUsage>>,

    /// Historical usage data with timestamps
    history: Arc<RwLock<Vec<ResourceUsage>>>,

    /// Maximum history entries to keep
    max_history: usize,

    /// Background task handle
    #[cfg(feature = "tokio")]
    task_handle: Option<tokio::task::JoinHandle<()>>,
    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    task_handle: Option<async_std::task::JoinHandle<()>>,
    #[cfg(not(any(feature = "tokio", feature = "async-std")))]
    task_handle: Option<std::thread::JoinHandle<()>>,

    /// The process ID being monitored (usually self)
    pid: u32,
}

impl ResourceTracker {
    /// Creates a new `ResourceTracker` with the given sampling interval
    #[must_use]
    pub fn new(sample_interval: Duration) -> Self {
        // Initialize with default values
        let initial_usage = ResourceUsage::new(0, 0.0, 0);

        Self {
            sample_interval,
            current_usage: Arc::new(RwLock::new(initial_usage)),
            history: Arc::new(RwLock::new(Vec::new())),
            max_history: 60, // Default to 1 minute at 1 second intervals
            task_handle: None,
            pid: std::process::id(),
        }
    }

    /// Sets the maximum history entries to keep
    #[must_use]
    pub const fn with_max_history(mut self, max_entries: usize) -> Self {
        self.max_history = max_entries;
        self
    }

    /// Starts the resource tracking in the background
    /// Starts the resource tracker's background sampling task
    ///
    /// # Errors
    ///
    /// Returns an error if the process ID cannot be determined or
    /// if there's an issue with the system APIs when gathering resource metrics
    #[cfg(all(feature = "tokio", not(feature = "async-std")))]
    pub fn start(&mut self) -> Result<()> {
        if self.task_handle.is_some() {
            return Ok(()); // Already started
        }

        let sample_interval = self.sample_interval;
        let usage_history = Arc::clone(&self.history);
        let current_usage = Arc::clone(&self.current_usage);
        let pid = self.pid;
        let max_history = self.max_history;

        let handle = tokio::spawn(async move {
            let mut interval_timer = time::interval(sample_interval);
            let mut last_cpu_time = 0.0;
            let mut last_timestamp = Instant::now();

            loop {
                interval_timer.tick().await;

                // Get current resource usage
                if let Ok(usage) = ResourceTracker::sample_resource_usage(
                    pid,
                    &mut last_cpu_time,
                    &mut last_timestamp,
                ) {
                    // Update current usage
                    if let Ok(mut current) = current_usage.write() {
                        *current = usage.clone();
                    }

                    // Update history
                    if let Ok(mut hist) = usage_history.write() {
                        hist.push(usage);

                        // Trim history if needed
                        if hist.len() > max_history {
                            hist.remove(0);
                        }
                    }
                }
            }
        });

        self.task_handle = Some(handle);
        Ok(())
    }

    /// Starts the resource tracking
    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    pub fn start(&mut self) -> Result<()> {
        if self.task_handle.is_some() {
            return Ok(()); // Already started
        }

        let sample_interval = self.sample_interval;
        let usage_history = Arc::clone(&self.history);
        let current_usage = Arc::clone(&self.current_usage);
        let pid = self.pid;
        let max_history = self.max_history; // Clone max_history to use inside async block

        let handle = async_std::task::spawn(async move {
            let mut last_cpu_time = 0.0;
            let mut last_timestamp = Instant::now();

            loop {
                async_std::task::sleep(sample_interval).await;

                // Get current resource usage
                if let Ok(usage) = ResourceTracker::sample_resource_usage(
                    pid,
                    &mut last_cpu_time,
                    &mut last_timestamp,
                ) {
                    // Update current usage
                    if let Ok(mut current) = current_usage.write() {
                        *current = usage.clone();
                    }

                    // Update history
                    if let Ok(mut hist) = usage_history.write() {
                        hist.push(usage);

                        // Trim history if needed
                        if hist.len() > max_history {
                            hist.remove(0);
                        }
                    }
                }
            }
        });

        self.task_handle = Some(handle);
        Ok(())
    }

    /// Stops the resource tracker, cancelling any ongoing monitoring task.
    ///
    /// For tokio, this aborts the task and awaits its completion.
    #[cfg(all(feature = "tokio", not(feature = "async-std")))]
    pub async fn stop(&mut self) {
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
    }

    /// Stops the resource tracker, cancelling any ongoing monitoring task.
    ///
    /// For async-std, this simply drops the JoinHandle which cancels the task.
    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    pub fn stop(&mut self) {
        // Just drop the handle, which will cancel the task on async-std
        self.task_handle.take();
    }

    /// Returns the current resource usage
    #[must_use]
    pub fn current_usage(&self) -> ResourceUsage {
        self.current_usage
            .read()
            .map_or_else(|_| ResourceUsage::new(0, 0.0, 0), |usage| usage.clone())
    }

    /// Returns a copy of the resource usage history
    #[must_use]
    pub fn history(&self) -> Vec<ResourceUsage> {
        self.history
            .read()
            .map_or_else(|_| Vec::new(), |history| history.clone())
    }

    /// Samples the resource usage for the given process ID
    #[allow(unused_variables, dead_code)]
    #[allow(clippy::needless_pass_by_ref_mut)]
    fn sample_resource_usage(
        pid: u32,
        last_cpu_time: &mut f64,
        last_timestamp: &mut Instant,
    ) -> Result<ResourceUsage> {
        #[cfg(target_os = "linux")]
        {
            // On Linux, read from /proc filesystem
            let memory = Self::get_memory_linux(pid)?;
            let (cpu, threads) = Self::get_cpu_linux(pid, last_cpu_time, last_timestamp)?;
            Ok(ResourceUsage::new(memory, cpu, threads))
        }

        #[cfg(target_os = "macos")]
        {
            // On macOS, use ps command
            let memory = Self::get_memory_macos(pid)?;
            let (cpu, threads) = Self::get_cpu_macos(pid)?;
            Ok(ResourceUsage::new(memory, cpu, threads))
        }

        #[cfg(target_os = "windows")]
        {
            // On Windows, use Windows API
            let memory = Self::get_memory_windows(pid)?;
            let (cpu, threads) = Self::get_cpu_windows(pid, last_cpu_time, last_timestamp)?;
            Ok(ResourceUsage::new(memory, cpu, threads))
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            // Default placeholder for unsupported platforms
            Ok(ResourceUsage::new(0, 0.0, 0))
        }
    }

    #[cfg(target_os = "linux")]
    fn get_memory_linux(pid: u32) -> Result<u64> {
        // Read memory information from /proc/[pid]/status
        let path = format!("/proc/{pid}/status");
        let file = File::open(&path).map_err(|e| {
            Error::io_with_source(format!("Failed to open {path} for memory stats"), e)
        })?;

        let reader = BufReader::new(file);
        let mut memory_bytes = 0;

        for line in reader.lines() {
            let line = line.map_err(|e| {
                Error::io_with_source("Failed to read process memory stats".to_string(), e)
            })?;

            // VmRSS gives the resident set size
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if !parts.is_empty() {
                    if let Ok(kb) = parts[1].parse::<u64>() {
                        memory_bytes = kb * 1024;
                        break;
                    }
                }
            }
        }

        Ok(memory_bytes)
    }

    #[cfg(target_os = "linux")]
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::similar_names
    )]
    fn get_cpu_linux(
        pid: u32,
        last_cpu_time: &mut f64,
        last_timestamp: &mut Instant,
    ) -> Result<(f64, u32)> {
        // Read CPU information from /proc/[pid]/stat
        let path = format!("/proc/{pid}/stat");
        let file = File::open(&path).map_err(|e| {
            Error::io_with_source(format!("Failed to open {path} for CPU stats"), e)
        })?;

        let reader = BufReader::new(file);
        let mut cpu_percent = 0.0;
        let mut thread_count: u32 = 0;

        if let Ok(line) = reader.lines().next().ok_or_else(|| {
            Error::runtime("Failed to read CPU stats from proc filesystem".to_string())
        }) {
            let line = line.map_err(|e| {
                Error::io_with_source("Failed to read process CPU stats".to_string(), e)
            })?;

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 24 {
                // Parse thread count (field 20)
                thread_count = parts[19].parse::<u32>().unwrap_or(0);

                // Parse CPU times (fields 14-17: utime, stime, cutime, cstime)
                let utime = parts[13].parse::<f64>().unwrap_or(0.0);
                let stime = parts[14].parse::<f64>().unwrap_or(0.0);
                let child_user_time = parts[15].parse::<f64>().unwrap_or(0.0);
                let child_system_time = parts[16].parse::<f64>().unwrap_or(0.0);

                let current_cpu_time = utime + stime + child_user_time + child_system_time;
                let now = Instant::now();

                // Calculate CPU usage percentage
                if *last_timestamp != Instant::now() {
                    let time_diff = now.duration_since(*last_timestamp).as_secs_f64();
                    if time_diff > 0.0 {
                        // CPU usage is normalized by the number of cores
                        let num_cores = num_cpus::get() as f64;
                        let cpu_time_diff = current_cpu_time - *last_cpu_time;

                        // Convert jiffies to percentage
                        // In Linux, typically there are 100 jiffies per second
                        cpu_percent = (cpu_time_diff / 100.0) / time_diff * 100.0 / num_cores;
                    }
                }

                // Update last values
                *last_cpu_time = current_cpu_time;
                *last_timestamp = now;
            }
        }

        Ok((cpu_percent, thread_count))
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    fn get_memory_macos(pid: u32) -> Result<u64> {
        // Use ps command to get memory usage on macOS
        let output = Command::new("ps")
            .args(["-xo", "rss=", "-p", &pid.to_string()])
            .output()
            .map_err(|e| {
                Error::io_with_source(
                    "Failed to execute ps command for memory stats".to_string(),
                    e,
                )
            })?;

        let memory_kb = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u64>()
            .unwrap_or(0);

        Ok(memory_kb * 1024)
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    fn get_cpu_macos(pid: u32) -> Result<(f64, u32)> {
        // Get CPU percentage using ps
        let output = Command::new("ps")
            .args(["-xo", "%cpu,thcount=", "-p", &pid.to_string()])
            .output()
            .map_err(|e| {
                Error::io_with_source("Failed to execute ps command for CPU stats".to_string(), e)
            })?;

        let stats = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stats.split_whitespace().collect();

        let cpu_percent = if parts.is_empty() {
            0.0
        } else {
            parts[0].parse::<f64>().unwrap_or(0.0)
        };

        let thread_count = if parts.len() > 1 {
            parts[1].parse::<u32>().unwrap_or(0)
        } else {
            0
        };

        Ok((cpu_percent, thread_count))
    }

    #[cfg(all(target_os = "windows", feature = "windows-monitoring"))]
    #[allow(unsafe_code)]
    fn get_memory_windows(pid: u32) -> Result<u64> {
        let mut pmc = PROCESS_MEMORY_COUNTERS::default();
        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_INFORMATION, false, pid) }.map_err(|e| {
                Error::runtime_with_source(
                    format!("Failed to open process {} for memory stats", pid),
                    e,
                )
            })?;

        let result = unsafe {
            GetProcessMemoryInfo(
                handle,
                &mut pmc,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            )
        }
        .map_err(|e| {
            Error::runtime_with_source("Failed to get process memory info".to_string(), e)
        });

        unsafe { CloseHandle(handle) };
        result?;

        Ok(u64::try_from(pmc.WorkingSetSize).unwrap_or(pmc.WorkingSetSize as u64))
    }

    #[cfg(all(target_os = "windows", not(feature = "windows-monitoring")))]
    fn get_memory_windows(_pid: u32) -> Result<u64> {
        Err(Error::runtime(
            "Windows monitoring not enabled. Enable the 'windows-monitoring' feature".to_string(),
        ))
    }

    #[cfg(all(target_os = "windows", feature = "windows-monitoring"))]
    #[allow(unsafe_code, clippy::cast_precision_loss)]
    fn get_cpu_windows(
        pid: u32,
        last_cpu_time: &mut f64,
        last_timestamp: &mut Instant,
    ) -> Result<(f64, u32)> {
        let mut cpu_percent = 0.0;
        let mut thread_count = 0;

        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_INFORMATION, false, pid) }.map_err(|e| {
                Error::runtime_with_source(
                    format!("Failed to open process {} for CPU stats", pid),
                    e,
                )
            })?;

        // Get thread count by enumerating threads using ToolHelp snapshot
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0).map_err(|e| {
                Error::runtime_with_source(
                    "Failed to create ToolHelp snapshot for threads".to_string(),
                    e,
                )
            })?;

            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;

            if Thread32First(snapshot, &mut entry).is_ok() {
                loop {
                    if entry.th32OwnerProcessID == pid {
                        thread_count += 1;
                    }
                    if Thread32Next(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }

            let _ = CloseHandle(snapshot);
        }

        // Get CPU times
        let mut creation_time = FILETIME::default();
        let mut exit_time = FILETIME::default();
        let mut kernel_time = FILETIME::default();
        let mut user_time = FILETIME::default();

        let result = unsafe {
            GetProcessTimes(
                handle,
                &mut creation_time,
                &mut exit_time,
                &mut kernel_time,
                &mut user_time,
            )
        };

        if result.is_ok() {
            let kernel_ns = Self::filetime_to_ns(&kernel_time);
            let user_ns = Self::filetime_to_ns(&user_time);
            let total_time = (kernel_ns + user_ns) as f64 / 1_000_000_000.0; // Convert to seconds

            let now = Instant::now();
            if *last_timestamp != Instant::now() {
                let time_diff = now.duration_since(*last_timestamp).as_secs_f64();
                if time_diff > 0.0 {
                    let time_diff_cpu = total_time - *last_cpu_time;
                    let num_cores = num_cpus::get() as f64;

                    // Calculate CPU percentage
                    cpu_percent = (time_diff_cpu / time_diff) * 100.0 / num_cores;
                }
            }

            // Update last values
            *last_cpu_time = total_time;
            *last_timestamp = now;
        }

        unsafe { CloseHandle(handle) };

        Ok((cpu_percent, thread_count))
    }

    #[cfg(all(target_os = "windows", not(feature = "windows-monitoring")))]
    fn get_cpu_windows(
        _pid: u32,
        _last_cpu_time: &mut f64,
        _last_timestamp: &mut Instant,
    ) -> Result<(f64, u32)> {
        Err(Error::runtime(
            "Windows monitoring not enabled. Enable the 'windows-monitoring' feature".to_string(),
        ))
    }

    #[cfg(all(target_os = "windows", feature = "windows-monitoring"))]
    fn filetime_to_ns(ft: &windows::Win32::Foundation::FILETIME) -> u64 {
        // Convert Windows FILETIME to nanoseconds
        let high = (ft.dwHighDateTime as u64) << 32;
        let low = ft.dwLowDateTime as u64;
        // Windows ticks are 100ns intervals
        (high + low) * 100
    }
}

impl Drop for ResourceTracker {
    fn drop(&mut self) {
        if let Some(handle) = self.task_handle.take() {
            #[cfg(feature = "tokio")]
            handle.abort();
            // For async-std, dropping the handle is sufficient
            // as it cancels the associated task
            #[cfg(all(feature = "async-std", not(feature = "tokio")))]
            drop(handle); // Explicitly drop handle to ensure it's used
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[cfg(feature = "tokio")]
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_resource_tracker_creation() {
        let tracker = ResourceTracker::new(Duration::from_secs(1));
        assert_eq!(tracker.max_history, 60);
        assert_eq!(tracker.sample_interval, Duration::from_secs(1));
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    #[async_std::test]
    async fn test_resource_tracker_creation() {
        let tracker = ResourceTracker::new(Duration::from_secs(1));
        assert_eq!(tracker.max_history, 60);
        assert_eq!(tracker.sample_interval, Duration::from_secs(1));
    }

    #[cfg(feature = "tokio")]
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_resource_usage_methods() {
        let usage = ResourceUsage::new(1_048_576, 5.5, 4);
        assert_eq!(usage.memory_bytes(), 1_048_576);
        // Use a more reasonable epsilon for floating point comparisons
        let epsilon: f64 = 1e-6;
        assert!((usage.memory_mb() - 1.0).abs() < epsilon);
        assert!((usage.cpu_percent() - 5.5).abs() < epsilon);
        assert_eq!(usage.thread_count(), 4);
        assert!(usage.age() >= Duration::from_nanos(0));
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    #[async_std::test]
    async fn test_resource_usage_methods() {
        let usage = ResourceUsage::new(1_048_576, 5.5, 4);
        assert_eq!(usage.memory_bytes(), 1_048_576);
        // Use a more reasonable epsilon for floating point comparisons
        let epsilon: f64 = 1e-6;
        assert!((usage.memory_mb() - 1.0).abs() < epsilon);
        assert!((usage.cpu_percent() - 5.5).abs() < epsilon);
        assert_eq!(usage.thread_count(), 4);
        assert!(usage.age() >= Duration::from_nanos(0));
    }

    #[cfg(feature = "tokio")]
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_tracker_with_max_history() {
        let tracker = ResourceTracker::new(Duration::from_secs(1)).with_max_history(100);
        assert_eq!(tracker.max_history, 100);
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    #[async_std::test]
    async fn test_tracker_with_max_history() {
        let tracker = ResourceTracker::new(Duration::from_secs(1)).with_max_history(100);
        assert_eq!(tracker.max_history, 100);
    }
}
