//! Simple daemon example showing basic usage.

use proc_daemon::{Daemon, Config, ShutdownHandle};
use std::time::Duration;
use tracing::info;

async fn simple_worker(mut shutdown: ShutdownHandle) -> proc_daemon::Result<()> {
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

#[tokio::main]
async fn main() -> proc_daemon::Result<()> {
    let config = Config::new()?;
    
    Daemon::builder(config)
        .with_task("simple_worker", simple_worker)
        .run()
        .await
}
