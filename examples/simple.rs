//! Simple daemon example showing basic usage.

use proc_daemon::{Config, Daemon, Result};
use std::time::Duration;

// Hold a global events receiver for the demo when lock-free coordination is enabled
#[cfg(feature = "lockfree-coordination")]
use std::sync::OnceLock;
#[cfg(feature = "lockfree-coordination")]
static EVENTS_RX: OnceLock<
    proc_daemon::coord::chan::Receiver<proc_daemon::subsystem::SubsystemEvent>,
> = OnceLock::new();

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

#[cfg(all(feature = "tokio", feature = "lockfree-coordination"))]
async fn events_worker(mut shutdown: proc_daemon::shutdown::ShutdownHandle) -> Result<()> {
    use proc_daemon::coord::chan;
    use proc_daemon::subsystem::SubsystemEvent;
    use tracing::info;

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(Duration::from_millis(250)) => {
                if let Some(rx) = EVENTS_RX.get() {
                    match chan::try_recv(rx) {
                        Ok(SubsystemEvent::StateChanged { id, name, state, at: _ }) => {
                            info!(%id, %name, %state, "subsystem state changed");
                        }
                        Err(_) => {}
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(all(feature = "async-std", not(feature = "tokio")))]
async fn simple_worker(shutdown: proc_daemon::shutdown::ShutdownHandle) -> Result<()> {
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

    // Optional: demonstrate lock-free coordination facade
    #[cfg(feature = "lockfree-coordination")]
    {
        use proc_daemon::coord::chan;
        use tracing::info;

        let (tx, rx) = chan::unbounded::<&'static str>();

        // Producer
        tokio::spawn(async move {
            let _ = tx.send("hello-from-coordination");
        });

        // Non-blocking consumer (single probe for brevity)
        tokio::spawn(async move {
            match chan::try_recv(&rx) {
                Ok(msg) => info!("coordination message: {}", msg),
                Err(_e) => info!("no coordination message available yet"),
            }
        });
    }

    let builder = Daemon::builder(config);
    #[cfg(feature = "lockfree-coordination")]
    let builder = {
        use proc_daemon::subsystem::SubsystemManager;
        builder.with_subsystem_fn("enable_events", |mgr: &SubsystemManager| {
            mgr.enable_events();
            if let Some(rx) = mgr.subscribe_events() {
                let _ = EVENTS_RX.set(rx);
            }
            // register the polling worker to log events
            mgr.register_fn("events_worker", events_worker)
        })
    };

    builder
        .with_task("simple_worker", simple_worker)
        .run()
        .await
}

#[cfg(all(feature = "async-std", not(feature = "tokio")))]
#[async_std::main]
async fn main() -> proc_daemon::Result<()> {
    let config = Config::new()?;

    // Optional: demonstrate lock-free coordination facade
    #[cfg(feature = "lockfree-coordination")]
    {
        use proc_daemon::coord::chan;
        use tracing::info;

        let (tx, rx) = chan::unbounded::<&'static str>();

        // Producer
        async_std::task::spawn({
            let tx = tx.clone();
            async move {
                let _ = tx.send("hello-from-coordination");
            }
        });

        // Non-blocking consumer (single probe for brevity)
        async_std::task::spawn(async move {
            match chan::try_recv(&rx) {
                Ok(msg) => info!("coordination message: {}", msg),
                Err(_e) => info!("no coordination message available yet"),
            }
        });
    }

    let builder = Daemon::builder(config);
    #[cfg(feature = "lockfree-coordination")]
    {
        use proc_daemon::subsystem::SubsystemManager;
        builder = builder.with_subsystem_fn("enable_events", |mgr: &SubsystemManager| {
            mgr.enable_events();
            if let Some(rx) = mgr.subscribe_events() {
                let _ = EVENTS_RX.set(rx);
            }
            // register the polling worker to log events
            mgr.register_fn("events_worker", events_worker)
        });
    }

    builder
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
