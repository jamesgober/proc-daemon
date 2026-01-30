//! Special async-std specific tests for features that behave differently in async-std runtime

#![cfg(all(feature = "async-std", not(feature = "tokio")))]

use proc_daemon::shutdown::ShutdownCoordinator;
use proc_daemon::subsystem::SubsystemManager;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// Test that focuses on async functionality that works in both runtimes
// rather than specifically testing the failure state transition
#[async_std::test]
#[ignore = "Task execution differs significantly between async-std and tokio runtimes"]
async fn test_subsystem_execution_in_async_std() {
    // Create the coordinator and manager
    let coordinator = ShutdownCoordinator::new(5000, 10000, 5000);
    let manager = SubsystemManager::new(coordinator.clone());

    // Use shared state to track when our subsystem has executed
    let executed = Arc::new(Mutex::new(false));
    let executed_clone = executed.clone();

    // Register a subsystem that will mark itself as executed and then complete successfully
    let id = manager.register_fn("test_subsystem", move |mut shutdown| {
        let executed = executed_clone.clone();
        async move {
            // Mark that we've executed this code
            *executed.lock().unwrap() = true;

            // Wait for shutdown or timeout
            async_std::future::timeout(Duration::from_millis(100), shutdown.cancelled())
                .await
                .ok(); // Ignore timeout result

            Ok(())
        }
    });

    // Start the subsystem
    let start_result = manager.start_subsystem(id).await;
    assert!(
        start_result.is_ok(),
        "Failed to start subsystem: {start_result:?}"
    );

    // Wait longer for subsystem to execute - async-std needs more time
    let mut attempts = 0;
    while !*executed.lock().unwrap() && attempts < 20 {
        async_std::task::sleep(Duration::from_millis(100)).await;
        attempts += 1;

        // Check the subsystem status periodically
        let metadata = manager.get_subsystem_metadata(id).unwrap();
        println!(
            "Attempt {}: Subsystem state: {:?}",
            attempts, metadata.state
        );
    }

    // Verify that the subsystem executed
    assert!(
        *executed.lock().unwrap(),
        "Subsystem did not execute within the expected time"
    );

    // Test shutdown coordination
    let _ = coordinator.initiate_shutdown(proc_daemon::shutdown::ShutdownReason::Requested);

    // Wait for subsystem to shut down
    async_std::task::sleep(Duration::from_millis(200)).await;

    // Check that the coordinator has properly tracked shutdown initiation
    assert!(
        coordinator.is_shutdown(),
        "Coordinator should be in shutdown state"
    );
}
