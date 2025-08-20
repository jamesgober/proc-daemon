//! File-based locking mechanism to prevent multiple daemon instances.
//!
//! This module provides cross-platform file locking capabilities to ensure
//! only a single instance of a daemon runs at any given time.

use std::{fs::File, path::Path};
use fs2::FileExt;
use crate::error::{Error, Result};

/// File lock manager for ensuring single-instance daemon execution.
#[derive(Debug)]
pub struct InstanceLock {
    /// The lock file handle
    file: Option<File>,
    /// Path to the lock file
    path: String,
}

impl InstanceLock {
    /// Creates a new instance lock manager.
    ///
    /// # Arguments
    ///
    /// * `path` - Path where the lock file will be created
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let path_str = path.as_ref().to_string_lossy().to_string();
        Self {
            file: None,
            path: path_str,
        }
    }
    
    /// Attempts to acquire the lock.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the lock was successfully acquired
    /// * `Err` if the lock could not be acquired (possibly because another instance is running)
    ///   Acquires a lock on the file to ensure single-instance execution
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be created, opened, or locked
    pub fn lock(&mut self) -> Result<()> {
        // Create or open the lock file
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.path)
            .map_err(|e| Error::io_with_source(
                format!("Failed to open or create lock file at {}", self.path),
                e
            ))?;
        
        // Try to acquire an exclusive lock
        file.try_lock_exclusive()
            .map_err(|e| Error::runtime_with_source(
                format!("Failed to acquire exclusive lock on {}, another instance may be running", self.path),
                e
            ))?;
        
        // Store the locked file
        self.file = Some(file);
        Ok(())
    }
    
    /// Releases the lock if it was acquired.
    /// Releases the lock on the file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be unlocked
    pub fn unlock(&mut self) -> Result<()> {
        if let Some(file) = self.file.take() {
            // Release the lock by unlocking the file
            fs2::FileExt::unlock(&file)
                .map_err(|e| Error::io_with_source(
                    format!("Failed to release lock on file {}", self.path),
                    e
                ))?;
        }
        Ok(())
    }
    
    /// Checks if the lock is currently held by this instance.
    /// Checks if a lock is currently held
    #[must_use]
    pub const fn is_locked(&self) -> bool {
        self.file.is_some()
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        // Ensure the lock is released when the InstanceLock is dropped
        if self.is_locked() {
            let _ = self.unlock();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // std::fs removed as it's not used
    use tempfile::tempdir;
    
    #[test]
    fn test_lock_acquisition() {
        // Create a temporary directory for the lock file
        let dir = tempdir().expect("Failed to create temporary directory");
        let lock_path = dir.path().join("test.lock");
        
        // Create an instance lock
        let mut lock = InstanceLock::new(&lock_path);
        
        // Should be able to acquire the lock
        assert!(lock.lock().is_ok());
        assert!(lock.is_locked());
        
        // Create a second lock on the same file
        let mut lock2 = InstanceLock::new(&lock_path);
        
        // Should not be able to acquire the lock
        assert!(lock2.lock().is_err());
        assert!(!lock2.is_locked());
        
        // Release the first lock
        assert!(lock.unlock().is_ok());
        assert!(!lock.is_locked());
        
        // Now the second lock should be able to acquire it
        assert!(lock2.lock().is_ok());
        assert!(lock2.is_locked());
    }
    
    #[test]
    fn test_lock_drop() {
        // Create a temporary directory for the lock file
        let dir = tempdir().expect("Failed to create temporary directory");
        let lock_path = dir.path().join("drop_test.lock");
        
        {
            // Create and acquire lock in an inner scope
            let mut lock = InstanceLock::new(&lock_path);
            assert!(lock.lock().is_ok());
            // Lock goes out of scope here and should be automatically released
        }
        
        // Should be able to create and acquire a new lock on the same file
        let mut lock2 = InstanceLock::new(&lock_path);
        assert!(lock2.lock().is_ok());
    }
}
