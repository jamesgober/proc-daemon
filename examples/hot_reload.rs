//! Demonstrates configuration hot-reload with a live config snapshot.
//! Requires features: `tokio`, `config-watch` (optionally `mmap-config`, `toml`).

#[cfg(all(feature = "tokio", feature = "config-watch"))]
use proc_daemon::{Config, Daemon};

#[cfg(all(feature = "tokio", feature = "config-watch"))]
#[tokio::main]
async fn main() -> proc_daemon::Result<()> {
    // Build config with hot-reload enabled
    let config = Config::builder()
        .name("hot-reload-demo")
        .hot_reload(true)
        .build()?;

    // Build daemon to get access to the live config snapshot
    let daemon = Daemon::builder(config)
        .with_task("noop", |_shutdown| async { Ok(()) })
        .build()?;

    // Clone for the observer task
    let observer = daemon.clone();

    // Spawn a task that periodically prints out dynamic config values
    tokio::spawn(async move {
        loop {
            // Read the current snapshot atomically
            let cfg = observer.config_snapshot();
            tracing::info!(
                name = %cfg.name,
                json_logging = %cfg.is_json_logging(),
                worker_threads = %cfg.performance.worker_threads,
                "Observed live config snapshot"
            );
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    });

    // Run the daemon (keeps the watcher alive internally when hot_reload=true)
    daemon.run().await
}

#[cfg(not(all(feature = "tokio", feature = "config-watch")))]
fn main() {
    eprintln!(
        "This example requires features: tokio + config-watch (optionally: toml mmap-config)."
    );
}
