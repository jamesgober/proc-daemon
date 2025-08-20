use criterion::{black_box, criterion_group, criterion_main, Criterion};
use proc_daemon::{Config, Daemon, ShutdownHandle};
use std::time::Duration;

async fn noop_subsystem(_shutdown: ShutdownHandle) -> proc_daemon::Result<()> {
    // Immediately return - this is just for benchmarking overhead
    Ok(())
}

fn bench_daemon_creation(c: &mut Criterion) {
    let _rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("daemon_creation", |b| {
        b.iter(|| {
            let config = Config::new().unwrap();
            let daemon = Daemon::builder(config).without_signals().build().unwrap();
            black_box(daemon);
        });
    });
}

fn bench_subsystem_registration(c: &mut Criterion) {
    let _rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("subsystem_registration", |b| {
        b.iter(|| {
            let config = Config::new().unwrap();
            let daemon = Daemon::builder(config)
                .with_task("bench", noop_subsystem)
                .with_task("bench2", noop_subsystem)
                .with_task("bench3", noop_subsystem)
                .without_signals()
                .build()
                .unwrap();
            black_box(daemon);
        });
    });
}

fn bench_config_loading(c: &mut Criterion) {
    c.bench_function("config_loading", |b| {
        b.iter(|| {
            let config = Config::builder()
                .name("bench-daemon")
                .worker_threads(4)
                .shutdown_timeout(Duration::from_secs(5))
                .unwrap()
                .force_shutdown_timeout(Duration::from_secs(2))
                .unwrap()
                .build()
                .unwrap();
            black_box(config);
        });
    });
}

fn bench_shutdown_coordination(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("shutdown_coordination", |b| {
        b.iter_custom(|iters| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                for _ in 0..iters {
                    let config = Config::new().unwrap();
                    let daemon = Daemon::builder(config)
                        .with_task("quick", noop_subsystem)
                        .without_signals()
                        .build()
                        .unwrap();

                    // Immediately shutdown
                    daemon.shutdown();
                    black_box(daemon);
                }
                start.elapsed()
            })
        });
    });
}

#[cfg(feature = "metrics")]
fn bench_metrics_collection(c: &mut Criterion) {
    use proc_daemon::metrics::MetricsCollector;

    let collector = MetricsCollector::new();

    c.bench_function("metrics_counter_increment", |b| {
        b.iter(|| {
            collector.increment_counter("bench_counter", black_box(1));
        });
    });

    c.bench_function("metrics_gauge_set", |b| {
        b.iter(|| {
            collector.set_gauge("bench_gauge", black_box(42));
        });
    });

    c.bench_function("metrics_histogram_record", |b| {
        b.iter(|| {
            collector.record_histogram("bench_histogram", black_box(Duration::from_nanos(100)));
        });
    });

    c.bench_function("metrics_snapshot", |b| {
        b.iter(|| {
            let snapshot = collector.get_metrics();
            black_box(snapshot);
        });
    });
}

fn bench_error_creation(c: &mut Criterion) {
    use proc_daemon::Error;

    c.bench_function("error_creation", |b| {
        b.iter(|| {
            let err = Error::runtime(black_box("benchmark error"));
            black_box(err);
        });
    });

    c.bench_function("error_chain", |b| {
        b.iter(|| {
            let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");
            let err = Error::io_with_source(black_box("operation failed"), io_err);
            black_box(err);
        });
    });
}

criterion_group!(
    benches,
    bench_daemon_creation,
    bench_subsystem_registration,
    bench_config_loading,
    bench_shutdown_coordination,
    bench_error_creation
);

#[cfg(feature = "metrics")]
criterion_group!(metrics_benches, bench_metrics_collection);

#[cfg(feature = "metrics")]
criterion_main!(benches, metrics_benches);

#[cfg(not(feature = "metrics"))]
criterion_main!(benches);
