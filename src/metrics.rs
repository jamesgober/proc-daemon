//! Metrics collection and monitoring for proc-daemon.
//!
//! This module provides optional metrics collection capabilities for monitoring
//! daemon performance, subsystem health, and resource usage.

use crate::pool::StringPool;
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// Metrics error handling

/// Metrics collector for daemon monitoring.
#[derive(Debug, Clone)]
pub struct MetricsCollector {
    inner: Arc<MetricsInner>,
    /// String pool for metric names to avoid allocations on hot paths
    string_pool: Arc<StringPool>,
}

impl MetricsSnapshot {
    /// Render metrics in Prometheus exposition format (text/plain; version=0.0.4)
    #[must_use]
    pub fn render_prometheus(&self) -> String {
        let mut out = String::with_capacity(1024);
        // Uptime
        out.push_str("# HELP proc_uptime_seconds Daemon uptime in seconds\n");
        out.push_str("# TYPE proc_uptime_seconds gauge\n");
        let _ = writeln!(out, "proc_uptime_seconds {}", self.uptime.as_secs_f64());

        // Gauges
        for (k, v) in &self.gauges {
            out.push('#');
            out.push_str(" TYPE ");
            out.push_str(k);
            out.push_str(" gauge\n");
            out.push_str(k);
            out.push(' ');
            out.push_str(&v.to_string());
            out.push('\n');
        }

        // Counters
        for (k, v) in &self.counters {
            out.push('#');
            out.push_str(" TYPE ");
            out.push_str(k);
            out.push_str(" counter\n");
            out.push_str(k);
            out.push(' ');
            out.push_str(&v.to_string());
            out.push('\n');
        }

        // Histograms (expose count and sum of seconds)
        for (k, durations) in &self.histograms {
            let count = durations.len() as u64;
            let sum: f64 = durations.iter().map(std::time::Duration::as_secs_f64).sum();
            let count_name = format!("{k}_count");
            let sum_name = format!("{k}_sum");

            out.push('#');
            out.push_str(" TYPE ");
            out.push_str(&count_name);
            out.push_str(" counter\n");
            out.push_str(&count_name);
            out.push(' ');
            out.push_str(&count.to_string());
            out.push('\n');

            out.push('#');
            out.push_str(" TYPE ");
            out.push_str(&sum_name);
            out.push_str(" counter\n");
            out.push_str(&sum_name);
            out.push(' ');
            out.push_str(&sum.to_string());
            out.push('\n');
        }

        out
    }
}

#[derive(Debug)]
struct MetricsInner {
    counters: RwLock<HashMap<String, AtomicU64>>,
    gauges: RwLock<HashMap<String, AtomicU64>>,
    histograms: RwLock<HashMap<String, VecDeque<Duration>>>,
    max_histogram_samples: usize,
    start_time: Instant,
}

impl MetricsCollector {
    const DEFAULT_MAX_HISTOGRAM_SAMPLES: usize = 2048;

    /// Create a new metrics collector.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MetricsInner {
                counters: RwLock::new(HashMap::new()),
                gauges: RwLock::new(HashMap::new()),
                histograms: RwLock::new(HashMap::new()),
                max_histogram_samples: Self::DEFAULT_MAX_HISTOGRAM_SAMPLES,
                start_time: Instant::now(),
            }),
            // Create a string pool optimized for metric names (typically short strings)
            string_pool: Arc::new(StringPool::new(50, 200, 64)),
        }
    }

    /// Increment a counter by the given value.
    pub fn increment_counter(&self, name: &str, value: u64) {
        // Fast path: try read lock first
        let counters = self.inner.counters.read();
        if let Some(counter) = counters.get(name) {
            counter.fetch_add(value, Ordering::Release);
            return;
        }
        drop(counters);

        // Slow path: need to create metric (happens once per unique metric)
        let pooled_name = self.string_pool.get_with_value(name);
        let mut counters = self.inner.counters.write();
        counters
            .entry(pooled_name.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(value, Ordering::Release);
    }

    /// Set a gauge to the given value.
    pub fn set_gauge(&self, name: &str, value: u64) {
        // Fast path: try read lock first
        let gauges = self.inner.gauges.read();
        if let Some(gauge) = gauges.get(name) {
            gauge.store(value, Ordering::Relaxed);
            return;
        }
        drop(gauges);

        // Slow path: need to create metric (happens once per unique metric)
        let pooled_name = self.string_pool.get_with_value(name);
        let mut gauges = self.inner.gauges.write();
        gauges
            .entry(pooled_name.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .store(value, Ordering::Relaxed);
    }

    /// Record a histogram value.
    pub fn record_histogram(&self, name: &str, duration: Duration) {
        // Use string pool to avoid allocation
        let pooled_name = self.string_pool.get_with_value(name);
        let mut histograms = self.inner.histograms.write();
        let max_samples = self.inner.max_histogram_samples;
        let entry = histograms
            .entry(pooled_name.to_string())
            .or_insert_with(|| VecDeque::with_capacity(64));

        if entry.len() >= max_samples {
            let _ = entry.pop_front();
        }
        entry.push_back(duration);
        drop(histograms);
    }

    /// Get current metric values.
    #[must_use]
    pub fn get_metrics(&self) -> MetricsSnapshot {
        let counters: HashMap<String, u64> = self
            .inner
            .counters
            .read()
            .iter()
            .map(|(k, v)| (k.clone(), v.load(Ordering::Relaxed)))
            .collect();

        let gauges: HashMap<String, u64> = self
            .inner
            .gauges
            .read()
            .iter()
            .map(|(k, v)| (k.clone(), v.load(Ordering::Relaxed)))
            .collect();

        let histograms: HashMap<String, Vec<Duration>> = self
            .inner
            .histograms
            .read()
            .iter()
            .map(|(k, v)| (k.clone(), v.iter().copied().collect()))
            .collect();

        MetricsSnapshot {
            uptime: self.inner.start_time.elapsed(),
            counters,
            gauges,
            histograms,
            timestamp: Instant::now(),
        }
    }

    /// Reset all metrics.
    pub fn reset(&self) {
        self.inner.counters.write().clear();
        self.inner.gauges.write().clear();
        self.inner.histograms.write().clear();
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of current metrics.
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    /// Daemon uptime
    pub uptime: Duration,
    /// Counter metrics
    pub counters: HashMap<String, u64>,
    /// Gauge metrics
    pub gauges: HashMap<String, u64>,
    /// Histogram metrics
    pub histograms: HashMap<String, Vec<Duration>>,
    /// Timestamp when snapshot was taken
    pub timestamp: Instant,
}

/// Timer for measuring operation duration.
#[derive(Debug)]
pub struct Timer {
    collector: MetricsCollector,
    name: Arc<str>, // Use Arc<str> instead of String to avoid clone during drop
    start: Instant,
}

impl Timer {
    /// Create a new timer for the given metric.
    #[must_use]
    pub fn new(collector: MetricsCollector, name: impl AsRef<str>) -> Self {
        // Create an Arc<str> directly from the input name
        // This avoids holding a reference to the collector's string pool
        let name_arc: Arc<str> = Arc::from(name.as_ref());

        Self {
            collector,
            name: name_arc,
            start: Instant::now(),
        }
    }

    /// Stop the timer and record the duration.
    pub fn stop(self) {
        let duration = self.start.elapsed();
        self.collector
            .record_histogram(self.name.as_ref(), duration);
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        // Use the Arc<str> directly to avoid allocation
        self.collector
            .record_histogram(self.name.as_ref(), duration);
    }
}

/// Macro for timing code blocks.
#[macro_export]
macro_rules! time_block {
    ($collector:expr, $metric:expr, $block:block) => {{
        // Pass metric directly to avoid to_string() allocation
        let _timer = $crate::metrics::Timer::new($collector.clone(), $metric);
        $block
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new();

        // Test counter
        collector.increment_counter("test_counter", 5);
        collector.increment_counter("test_counter", 3);

        // Test gauge
        collector.set_gauge("test_gauge", 42);

        // Test histogram
        collector.record_histogram("test_histogram", Duration::from_millis(100));
        collector.record_histogram("test_histogram", Duration::from_millis(200));

        let snapshot = collector.get_metrics();

        assert_eq!(snapshot.counters.get("test_counter"), Some(&8));
        assert_eq!(snapshot.gauges.get("test_gauge"), Some(&42));
        assert_eq!(snapshot.histograms.get("test_histogram").unwrap().len(), 2);
    }

    #[test]
    fn test_timer() {
        let collector = MetricsCollector::new();

        {
            let _timer = Timer::new(collector.clone(), "test_timer");
            std::thread::sleep(Duration::from_millis(10));
        }

        let snapshot = collector.get_metrics();
        let durations = snapshot.histograms.get("test_timer").unwrap();
        assert_eq!(durations.len(), 1);
        assert!(durations[0] >= Duration::from_millis(10));
    }
}
