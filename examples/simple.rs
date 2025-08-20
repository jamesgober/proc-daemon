//! Simple daemon example showing basic usage.

use proc_daemon::{Config, Daemon, Result};
use std::time::Duration;

#[cfg(feature = "tokio")]
async fn simple_worker(mut shutdown: proc_daemon::shutdown::ShutdownHandle) -> Result<()> {
    use tracing::info;
    let mut counter = 0;

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                info!("Worker shutting down after {} iterations", counter);
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                counter += 1;
                info!("Worker iteration {}", counter);
            }
        }
    }

    Ok(())
}

#[cfg(all(feature = "async-std", not(feature = "tokio")))]
async fn simple_worker(mut shutdown: proc_daemon::shutdown::ShutdownHandle) -> Result<()> {
    use tracing::info;
    let mut counter = 0;

    loop {
        // Check if shutdown is requested
        if shutdown.is_shutdown() {
            info!("Worker shutting down after {} iterations", counter);
            break;
        }

        // Sleep for a bit
        async_std::task::sleep(Duration::from_secs(1)).await;
        counter += 1;
        info!("Worker iteration {}", counter);
    }

    Ok(())
}

#[cfg(feature = "tokio")]
#[tokio::main]
async fn main() -> proc_daemon::Result<()> {
    let config = Config::new()?;

    Daemon::builder(config)
        .with_task("simple_worker", simple_worker)
        .run()
        .await
}

#[cfg(all(feature = "async-std", not(feature = "tokio")))]
#[async_std::main]
async fn main() -> proc_daemon::Result<()> {
    let config = Config::new()?;

    Daemon::builder(config)
        .with_task("simple_worker", simple_worker)
        .run()
        .await
}

#[cfg(not(any(feature = "tokio", feature = "async-std")))]
fn main() {
    eprintln!(
        "This example requires a runtime feature. Enable either 'tokio' or 'async-std' features."
    );
}
