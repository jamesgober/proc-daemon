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
//! ```rust
//! use proc_daemon::resources::{ResourceTracker, ResourceUsage};
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> proc_daemon::Result<()> {
//!     // Create a new resource tracker sampling every second
//!     let mut tracker = ResourceTracker::new(Duration::from_secs(1));
//!     
//!     // Start tracking
//!     tracker.start().await?;
//!     
//!     // Get the current resource usage
//!     let usage: ResourceUsage = tracker.current_usage();
//!     println!("Memory: {}MB, CPU: {}%", usage.memory_mb(), usage.cpu_percent());
//!     
//!     // Stop tracking when done
//!     tracker.stop().await;
//!     Ok(())
//! }
//! ```

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::time;
use tokio::task::JoinHandle;
use crate::error::{Error, Result};

#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(target_os = "linux")]
use std::io::{BufRead, BufReader};

#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(target_os = "windows")]
use windows::Win32::System::ProcessStatus;

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
    pub fn memory_mb(&self) -> f64 {
        // Convert to f64 with proper conversion factor to avoid precision loss
        f64::from(u32::try_from(self.memory_bytes & 0xFFFF_FFFF).unwrap_or(0)) +
            (f64::from(u32::try_from((self.memory_bytes >> 32) & 0xFFFF_FFFF).unwrap_or(0)) * 4_294_967_296.0) / 1_048_576.0
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
    task_handle: Option<JoinHandle<()>>,
    
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
    pub fn start(&mut self) -> Result<()> {
        // Don't start if already running
        if self.task_handle.is_some() {
            return Ok(());
        }
        
        let current_usage = self.current_usage.clone();
        let history = self.history.clone();
        let max_history = self.max_history;
        let interval = self.sample_interval;
        let pid = self.pid;
        
        // Create and spawn the background task
        let handle = tokio::spawn(async move {
            let mut interval_timer = time::interval(interval);
            let mut last_cpu_time = 0.0;
            let mut last_timestamp = Instant::now();
            
            loop {
                interval_timer.tick().await;
                
                // Get current resource usage
                if let Ok(usage) = Self::sample_resource_usage(pid, &mut last_cpu_time, &mut last_timestamp) {
                    // Update current usage
                    if let Ok(mut current) = current_usage.write() {
                        *current = usage.clone();
                    }
                    
                    // Update history
                    if let Ok(mut hist) = history.write() {
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
    
    /// Stops the resource tracking
    pub async fn stop(&mut self) {
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
    }
    
    /// Returns the current resource usage
    #[must_use]
    pub fn current_usage(&self) -> ResourceUsage {
        self.current_usage.read().map_or_else(
            |_| ResourceUsage::new(0, 0.0, 0),
            |usage| usage.clone()
        )
    }
    
    /// Returns a copy of the resource usage history
    #[must_use]
    pub fn history(&self) -> Vec<ResourceUsage> {
        self.history.read().map_or_else(
            |_| Vec::new(),
            |history| history.clone()
        )
    }
    
    /// Samples the resource usage for the given process ID
    fn sample_resource_usage(
        pid: u32, 
        _last_cpu_time: &mut f64, 
        _last_timestamp: &mut Instant
    ) -> Result<ResourceUsage> {
        #[cfg(target_os = "linux")]
        {
            // On Linux, read from /proc filesystem
            let memory = Self::get_memory_linux(pid)?;
            let (cpu, threads) = Self::get_cpu_linux(pid, _last_cpu_time, _last_timestamp)?;
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
            let (cpu, threads) = Self::get_cpu_windows(pid, _last_cpu_time, _last_timestamp)?;
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
        let path = format!("/proc/{}/status", pid);
        let file = File::open(&path)
            .map_err(|e| Error::io_with_source(
                format!("Failed to open {} for memory stats", path),
                e
            ))?;
        
        let reader = BufReader::new(file);
        let mut memory_bytes = 0;
        
        for line in reader.lines() {
            let line = line.map_err(|e| Error::io_with_source(
                "Failed to read process memory stats".to_string(),
                e
            ))?;
            
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
    fn get_cpu_linux(pid: u32, last_cpu_time: &mut f64, last_timestamp: &mut Instant) -> Result<(f64, u32)> {
        // Read CPU information from /proc/[pid]/stat
        let path = format!("/proc/{}/stat", pid);
        let file = File::open(&path)
            .map_err(|e| Error::io_with_source(
                format!("Failed to open {} for CPU stats", path),
                e
            ))?;
        
        let reader = BufReader::new(file);
        let mut cpu_percent = 0.0;
        let mut thread_count = 0;
        
        if let Ok(line) = reader.lines().next().ok_or_else(|| Error::runtime(
            "Failed to read CPU stats from proc filesystem".to_string()
        )) {
            let line = line.map_err(|e| Error::io_with_source(
                "Failed to read process CPU stats".to_string(),
                e
            ))?;
            
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 24 {
                // Parse thread count (field 20)
                thread_count = parts[19].parse::<u32>().unwrap_or(0);
                
                // Parse CPU times (fields 14-17: utime, stime, cutime, cstime)
                let utime = parts[13].parse::<f64>().unwrap_or(0.0);
                let stime = parts[14].parse::<f64>().unwrap_or(0.0);
                let cutime = parts[15].parse::<f64>().unwrap_or(0.0);
                let cstime = parts[16].parse::<f64>().unwrap_or(0.0);
                
                let current_cpu_time = utime + stime + cutime + cstime;
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
    fn get_memory_macos(pid: u32) -> Result<u64> {
        // Use ps command to get memory usage on macOS
        let output = Command::new("ps")
            .args(["-xo", "rss=", "-p", &pid.to_string()])
            .output()
            .map_err(|e| Error::io_with_source(
                "Failed to execute ps command for memory stats".to_string(),
                e
            ))?;
        
        let memory_kb = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u64>()
            .unwrap_or(0);
        
        Ok(memory_kb * 1024)
    }
    
    #[cfg(target_os = "macos")]
    fn get_cpu_macos(pid: u32) -> Result<(f64, u32)> {
        // Get CPU percentage using ps
        let output = Command::new("ps")
            .args(["-xo", "%cpu,thcount=", "-p", &pid.to_string()])
            .output()
            .map_err(|e| Error::io_with_source(
                "Failed to execute ps command for CPU stats".to_string(),
                e
            ))?;
        
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
    
    #[cfg(target_os = "windows")]
    fn get_memory_windows(pid: u32) -> Result<u64> {
        use windows::Win32::System::ProcessStatus::GetProcessMemoryInfo;
        use windows::Win32::System::Threading::OpenProcess;
        use windows::Win32::System::Threading::PROCESS_QUERY_INFORMATION;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::ProcessStatus::PROCESS_MEMORY_COUNTERS;
        
        let mut pmc = PROCESS_MEMORY_COUNTERS::default();
        let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION, false, pid) }
            .map_err(|e| Error::runtime_with_source(
                format!("Failed to open process {} for memory stats", pid),
                e
            ))?;
        
        let result = unsafe { GetProcessMemoryInfo(handle, &mut pmc, std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32) }
            .map_err(|e| Error::runtime_with_source(
                "Failed to get process memory info".to_string(),
                e
            ));
            
        unsafe { CloseHandle(handle) };
        result?;
        
        Ok(pmc.WorkingSetSize)
    }
    
    #[cfg(target_os = "windows")]
    fn get_cpu_windows(pid: u32, last_cpu_time: &mut f64, last_timestamp: &mut Instant) -> Result<(f64, u32)> {
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION};
        use windows::Win32::System::Threading::GetProcessTimes;
        use windows::Win32::Foundation::{FILETIME, CloseHandle};
        
        let mut cpu_percent = 0.0;
        let mut thread_count = 0;
        
        let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION, false, pid) }
            .map_err(|e| Error::runtime_with_source(
                format!("Failed to open process {} for CPU stats", pid),
                e
            ))?;
        
        // Get thread count
        let mut system_info = std::mem::zeroed::<ProcessStatus::SYSTEM_PROCESS_INFORMATION>();
        let status = unsafe { ProcessStatus::NtQuerySystemInformation(
            ProcessStatus::SystemProcessInformation,
            &mut system_info as *mut _ as *mut _,
            std::mem::size_of::<ProcessStatus::SYSTEM_PROCESS_INFORMATION>() as u32,
            std::ptr::null_mut(),
        ) };
        
        if status >= 0 {
            thread_count = system_info.NumberOfThreads;
        }
        
        // Get CPU times
        let mut creation_time = FILETIME::default();
        let mut exit_time = FILETIME::default();
        let mut kernel_time = FILETIME::default();
        let mut user_time = FILETIME::default();
        
        let result = unsafe { GetProcessTimes(
            handle,
            &mut creation_time,
            &mut exit_time,
            &mut kernel_time,
            &mut user_time,
        ) };
        
        if result.as_bool() {
            let kernel_ns = filetime_to_ns(&kernel_time);
            let user_ns = filetime_to_ns(&user_time);
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
    
    #[cfg(target_os = "windows")]
    fn filetime_to_ns(ft: &FILETIME) -> u64 {
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
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[tokio::test]
    async fn test_resource_tracker_creation() {
        let tracker = ResourceTracker::new(Duration::from_secs(1));
        assert_eq!(tracker.max_history, 60);
        assert_eq!(tracker.sample_interval, Duration::from_secs(1));
    }
    
    #[tokio::test]
    async fn test_resource_usage_methods() {
        let usage = ResourceUsage::new(1_048_576, 5.5, 4);
        assert_eq!(usage.memory_bytes(), 1_048_576);
        assert_eq!(usage.memory_mb(), 1.0);
        assert_eq!(usage.cpu_percent(), 5.5);
        assert_eq!(usage.thread_count(), 4);
        assert!(usage.age() >= Duration::from_nanos(0));
    }
    
    #[tokio::test]
    async fn test_tracker_with_max_history() {
        let tracker = ResourceTracker::new(Duration::from_secs(1))
            .with_max_history(100);
        assert_eq!(tracker.max_history, 100);
    }
}
