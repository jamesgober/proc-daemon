//! Cross-platform signal handling for graceful daemon shutdown.
//!
//! This module provides a unified interface for handling shutdown signals
//! across different platforms, abstracting away the platform-specific
//! differences between Unix signals and Windows console events.

use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, info, warn};

use crate::ShutdownReason;

// No need for these imports
// use crate::config::Config;
// use crate::daemon::Daemon;
use crate::error::{Error, ErrorCode, Result};
use crate::shutdown::ShutdownCoordinator;

/// Cross-platform signal handler that coordinates shutdown.
#[derive(Debug)]
pub struct SignalHandler {
    shutdown_coordinator: ShutdownCoordinator,
    handling_signals: AtomicBool,
}

impl SignalHandler {
    /// Create a new signal handler.
    #[must_use]
    pub const fn new(shutdown_coordinator: ShutdownCoordinator) -> Self {
        Self {
            shutdown_coordinator,
            handling_signals: AtomicBool::new(false),
        }
    }

    /// Start handling signals for graceful shutdown.
    /// This will register signal handlers and wait for shutdown signals.
    ///
    /// # Errors
    ///
    /// Returns an error if signal handling is already active or if there's a problem
    /// registering signal handlers on the platform.
    pub async fn handle_signals(&self) -> Result<()> {
        if self
            .handling_signals
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(Error::invalid_state("Signal handling already started"));
        }

        info!("Starting signal handler");

        // Platform-specific signal handling
        #[cfg(unix)]
        {
            return self.handle_unix_signals().await;
        }

        #[cfg(windows)]
        {
            return self.handle_windows_signals().await;
        }
    }

    /// Stop signal handling.
    pub fn stop(&self) {
        self.handling_signals.store(false, Ordering::Release);
        debug!("Signal handling stopped");
    }

    /// Check if signal handling is active.
    pub fn is_handling(&self) -> bool {
        self.handling_signals.load(Ordering::Acquire)
    }
}

// Unix-specific signal handling implementation
#[cfg(unix)]
impl SignalHandler {
    async fn handle_unix_signals(&self) -> Result<()> {
        #[cfg(all(feature = "tokio", not(feature = "async-std")))]
        {
            return self.handle_unix_signals_tokio().await;
        }

        #[cfg(all(feature = "async-std", not(feature = "tokio")))]
        {
            return self.handle_unix_signals_async_std().await;
        }

        #[cfg(not(any(feature = "tokio", feature = "async-std")))]
        {
            return Err(Error::runtime_with_code(
                ErrorCode::MissingRuntime,
                "No runtime available for signal handling",
            ));
        }

        #[cfg(all(feature = "tokio", feature = "async-std"))]
        {
            // Default to tokio when both are present
            return self.handle_unix_signals_tokio().await;
        }
    }

    #[cfg(feature = "tokio")]
    async fn handle_unix_signals_tokio(&self) -> Result<()> {
        use tokio::signal::unix::{signal, SignalKind};

        // Set up signal handlers for graceful shutdown
        let mut sigterm = signal(SignalKind::terminate()).map_err(|e| {
            Error::signal_with_number(format!("Failed to register SIGTERM handler: {e}"), 15)
        })?;

        let mut sigint = signal(SignalKind::interrupt()).map_err(|e| {
            Error::signal_with_number(format!("Failed to register SIGINT handler: {e}"), 2)
        })?;

        let mut sigquit = signal(SignalKind::quit()).map_err(|e| {
            Error::signal_with_number(format!("Failed to register SIGQUIT handler: {e}"), 3)
        })?;

        let mut sighup = signal(SignalKind::hangup()).map_err(|e| {
            Error::signal_with_number(format!("Failed to register SIGHUP handler: {e}"), 1)
        })?;

        info!("Unix signal handlers registered (SIGTERM, SIGINT, SIGQUIT, SIGHUP)");

        loop {
            tokio::select! {
                _ = sigterm.recv() => {
                    info!("Received SIGTERM, initiating graceful shutdown");
                    if self.shutdown_coordinator.initiate_shutdown(ShutdownReason::Signal(15)) {
                        break;
                    }
                }
                _ = sigint.recv() => {
                    info!("Received SIGINT (Ctrl+C), initiating graceful shutdown");
                    if self.shutdown_coordinator.initiate_shutdown(ShutdownReason::Signal(2)) {
                        break;
                    }
                }
                _ = sigquit.recv() => {
                    warn!("Received SIGQUIT, initiating immediate shutdown");
                    if self.shutdown_coordinator.initiate_shutdown(ShutdownReason::Signal(3)) {
                        break;
                    }
                }
                _ = sighup.recv() => {
                    info!("Received SIGHUP, could be used for config reload (initiating shutdown for now)");
                    if self.shutdown_coordinator.initiate_shutdown(ShutdownReason::Signal(1)) {
                        break;
                    }
                }
            }

            // Check if we should stop handling signals
            if !self.is_handling() {
                debug!("Signal handling stopped by request");
                break;
            }
        }

        Ok(())
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    async fn handle_unix_signals_async_std(&self) -> Result<()> {
        use std::time::Duration;

        // Use the signal-hook crate for safe signal handling
        let term = async_std::channel::bounded::<()>(1);
        let _int = async_std::channel::bounded::<i32>(1);

        // Register signal handlers safely using ctrlc for SIGTERM (15)
        let term_sender = term.0.clone();
        ctrlc::set_handler(move || {
            let _ = term_sender.try_send(());
        })?;

        info!("Unix signal handlers registered (SIGTERM, SIGINT)");

        // Poll for signals
        loop {
            // Wait for signals with a timeout
            let term_recv = term.1.clone();

            match async_std::future::timeout(Duration::from_millis(100), term_recv.recv()).await {
                Ok(Ok(_)) => {
                    info!("Received SIGTERM, initiating graceful shutdown");
                    if self
                        .shutdown_coordinator
                        .initiate_shutdown(ShutdownReason::Signal(15))
                    {
                        break;
                    }
                }
                _ => {
                    // Timeout or error, check if we should stop handling
                    if !self.is_handling() {
                        debug!("Signal handling stopped by request");
                        break;
                    }
                }
            }

            // Brief sleep to prevent tight loop
            async_std::task::sleep(Duration::from_millis(10)).await;
        }

        Ok(())
    }
}

// Windows-specific signal handling implementation
#[cfg(windows)]
impl SignalHandler {
    async fn handle_windows_signals(&self) -> Result<()> {
        #[cfg(feature = "tokio")]
        {
            self.handle_windows_signals_tokio().await
        }

        #[cfg(all(feature = "async-std", not(feature = "tokio")))]
        {
            self.handle_windows_signals_async_std().await
        }
    }

    #[cfg(feature = "tokio")]
    async fn handle_windows_signals_tokio(&self) -> Result<()> {
        use tokio::signal::windows::{ctrl_break, ctrl_c, ctrl_close, ctrl_shutdown};

        // Set up Windows console event handlers
        let mut ctrl_c_stream = ctrl_c()
            .map_err(|e| Error::signal(format!("Failed to register Ctrl+C handler: {}", e)))?;

        let mut ctrl_break_stream = ctrl_break()
            .map_err(|e| Error::signal(format!("Failed to register Ctrl+Break handler: {}", e)))?;

        let mut ctrl_close_stream = ctrl_close()
            .map_err(|e| Error::signal(format!("Failed to register Ctrl+Close handler: {}", e)))?;

        let mut ctrl_shutdown_stream = ctrl_shutdown()
            .map_err(|e| Error::signal(format!("Failed to register shutdown handler: {}", e)))?;

        info!("Windows console event handlers registered");

        loop {
            tokio::select! {
                _ = ctrl_c_stream.recv() => {
                    info!("Received Ctrl+C, initiating graceful shutdown");
                    if self.shutdown_coordinator.initiate_shutdown(ShutdownReason::Signal(2)) {
                        break;
                    }
                }
                _ = ctrl_break_stream.recv() => {
                    info!("Received Ctrl+Break, initiating graceful shutdown");
                    if self.shutdown_coordinator.initiate_shutdown(ShutdownReason::Signal(3)) {
                        break;
                    }
                }
                _ = ctrl_close_stream.recv() => {
                    warn!("Received console close event, initiating immediate shutdown");
                    if self.shutdown_coordinator.initiate_shutdown(ShutdownReason::Signal(1)) {
                        break;
                    }
                }
                _ = ctrl_shutdown_stream.recv() => {
                    warn!("Received system shutdown event, initiating immediate shutdown");
                    if self.shutdown_coordinator.initiate_shutdown(ShutdownReason::Signal(6)) {
                        break;
                    }
                }
            }

            // Check if we should stop handling signals
            if !self.is_handling() {
                debug!("Signal handling stopped by request");
                break;
            }
        }

        Ok(())
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    async fn handle_windows_signals_async_std(&self) -> Result<()> {
        use std::sync::atomic::{AtomicBool, Ordering};

        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let shutdown_flag_clone = Arc::clone(&shutdown_flag);

        // For Windows with async-std, we'll use a simpler approach
        // Install a basic Ctrl+C handler
        ctrlc::set_handler(move || {
            shutdown_flag_clone.store(true, Ordering::Release);
        })
        .map_err(|e| Error::signal(format!("Failed to set Ctrl+C handler: {}", e)))?;

        info!("Windows Ctrl+C handler registered");

        // Simple polling approach for async-std
        while self.is_handling() && !shutdown_flag.load(Ordering::Acquire) {
            async_std::task::sleep(std::time::Duration::from_millis(100)).await;
        }

        if shutdown_flag.load(Ordering::Acquire) {
            info!("Received Windows console event, initiating graceful shutdown");
            self.shutdown_coordinator
                .initiate_shutdown(ShutdownReason::Signal(2));
        }

        Ok(())
    }
}

// Simple signal handler for async-std on Unix
#[cfg(all(unix, feature = "async-std", not(feature = "tokio")))]
#[allow(dead_code)]
extern "C" fn handle_signal(_signal: libc::c_int) {
    // In a real implementation, we'd need to communicate back to the async task
    // For now, this is a placeholder
}

/// Helper function to get a human-readable description of a signal.
#[must_use]
pub const fn signal_description(signal: i32) -> &'static str {
    match signal {
        1 => "SIGHUP (Hangup)",
        2 => "SIGINT (Interrupt/Ctrl+C)",
        3 => "SIGQUIT (Quit)",
        6 => "SIGABRT (Abort)",
        9 => "SIGKILL (Kill - non-catchable)",
        15 => "SIGTERM (Terminate)",
        _ => "Unknown signal",
    }
}

/// Signal handling mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalHandling {
    /// Enabled - handle this signal
    Enabled,
    /// Disabled - do not handle this signal
    Disabled,
}

impl Default for SignalHandling {
    fn default() -> Self {
        Self::Disabled
    }
}

impl From<bool> for SignalHandling {
    fn from(value: bool) -> Self {
        if value {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }
}

impl From<SignalHandling> for bool {
    fn from(value: SignalHandling) -> Self {
        match value {
            SignalHandling::Enabled => true,
            SignalHandling::Disabled => false,
        }
    }
}

/// Configuration for signal handling
#[derive(Debug, Clone)]
pub struct SignalConfig {
    /// SIGTERM handling for graceful shutdown
    pub term: SignalHandling,
    /// SIGINT (Ctrl+C) handling for graceful shutdown  
    pub interrupt: SignalHandling,
    /// SIGQUIT handling for immediate shutdown
    pub quit: SignalHandling,
    /// SIGHUP handling for configuration reload
    pub hangup: SignalHandling,
    /// SIGUSR1 handling for custom actions
    pub user1: SignalHandling,
    /// SIGUSR2 handling for custom actions
    pub user2: SignalHandling,
    /// Custom signal handlers with signal number and description
    pub custom_handlers: Vec<(i32, String)>,
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            term: SignalHandling::Enabled,
            interrupt: SignalHandling::Enabled,
            quit: SignalHandling::Enabled,
            hangup: SignalHandling::Disabled, // Disabled by default
            user1: SignalHandling::Disabled,
            user2: SignalHandling::Disabled,
            // Pre-allocate with a reasonable capacity to avoid reallocation
            custom_handlers: Vec::with_capacity(4),
        }
    }
}

impl SignalConfig {
    /// Create a new signal configuration with defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable SIGHUP handling.
    #[must_use]
    pub const fn with_sighup(mut self) -> Self {
        self.hangup = SignalHandling::Enabled;
        self
    }

    /// Enable SIGUSR1 handling.
    #[must_use]
    pub const fn with_sigusr1(mut self) -> Self {
        self.user1 = SignalHandling::Enabled;
        self
    }

    /// Enable SIGUSR2 handling.
    #[must_use]
    pub const fn with_sigusr2(mut self) -> Self {
        self.user2 = SignalHandling::Enabled;
        self
    }

    /// Add a custom signal handler.
    #[must_use]
    pub fn with_custom_handler<S: Into<String>>(mut self, signal: i32, description: S) -> Self {
        self.custom_handlers.push((signal, description.into()));
        self
    }

    /// Disable SIGINT handling.
    #[must_use]
    pub const fn without_sigint(mut self) -> Self {
        self.interrupt = SignalHandling::Disabled;
        self
    }

    /// Disable SIGTERM handling.
    #[must_use]
    pub const fn without_sigterm(mut self) -> Self {
        self.term = SignalHandling::Disabled;
        self
    }

    /// Disable SIGQUIT handling.
    #[must_use]
    pub const fn without_sigquit(mut self) -> Self {
        self.quit = SignalHandling::Disabled;
        self
    }
}

/// Advanced signal handler with configurable signal handling.
#[derive(Debug)]
pub struct ConfigurableSignalHandler {
    #[allow(dead_code)]
    shutdown_coordinator: ShutdownCoordinator,
    config: SignalConfig,
    handling_signals: AtomicBool,
}

impl ConfigurableSignalHandler {
    /// Create a new configurable signal handler.
    #[must_use]
    pub const fn new(shutdown_coordinator: ShutdownCoordinator, config: SignalConfig) -> Self {
        Self {
            shutdown_coordinator,
            config,
            handling_signals: AtomicBool::new(false),
        }
    }

    /// Start handling configured signals.
    ///
    /// # Errors
    ///
    /// Returns an error if signal handling is already active or if there's a problem
    /// registering signal handlers on the platform.
    pub async fn handle_signals(&self) -> Result<()> {
        if self
            .handling_signals
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(Error::invalid_state("Signal handling already started"));
        }

        info!("Starting configurable signal handler");

        // Log which signals will be handled
        // Pre-allocate vector with exact capacity needed based on configuration
        let mut handled_signals = Vec::with_capacity(6); // Max 6 standard signals
        if bool::from(self.config.term) {
            handled_signals.push("SIGTERM");
        }
        if bool::from(self.config.interrupt) {
            handled_signals.push("SIGINT");
        }
        if bool::from(self.config.quit) {
            handled_signals.push("SIGQUIT");
        }
        if bool::from(self.config.hangup) {
            handled_signals.push("SIGHUP");
        }
        if bool::from(self.config.user1) {
            handled_signals.push("SIGUSR1");
        }
        if bool::from(self.config.user2) {
            handled_signals.push("SIGUSR2");
        }

        info!("Handling signals: {:?}", handled_signals);

        // Platform-specific implementation
        #[cfg(unix)]
        {
            self.handle_configured_unix_signals().await
        }

        #[cfg(windows)]
        {
            self.handle_configured_windows_signals().await
        }
    }

    #[cfg(unix)]
    async fn handle_configured_unix_signals(&self) -> Result<()> {
        #[cfg(feature = "tokio")]
        {
            use tokio::signal::unix::{signal, SignalKind};

            // For now, just handle SIGTERM and SIGINT with basic approach
            if bool::from(self.config.term) || bool::from(self.config.interrupt) {
                let mut sigterm = signal(SignalKind::terminate())?;
                let mut sigint = signal(SignalKind::interrupt())?;

                loop {
                    tokio::select! {
                        _ = sigterm.recv(), if bool::from(self.config.term) => {
                            info!("Received SIGTERM, initiating graceful shutdown");
                            if self.shutdown_coordinator.initiate_shutdown(ShutdownReason::Signal(15)) {
                                break;
                            }
                        }
                        _ = sigint.recv(), if bool::from(self.config.interrupt) => {
                            info!("Received SIGINT, initiating graceful shutdown");
                            if self.shutdown_coordinator.initiate_shutdown(ShutdownReason::Signal(2)) {
                                break;
                            }
                        }
                    }

                    if !self.handling_signals.load(Ordering::Acquire) {
                        break;
                    }
                }
            }
        }

        #[cfg(all(feature = "async-std", not(feature = "tokio")))]
        {
            // Simplified async-std implementation
            while self.handling_signals.load(Ordering::Acquire) {
                async_std::task::sleep(std::time::Duration::from_millis(100)).await;
            }
        }

        Ok(())
    }

    #[cfg(windows)]
    async fn handle_configured_windows_signals(&self) -> Result<()> {
        // Simplified Windows implementation
        #[cfg(feature = "tokio")]
        {
            use tokio::signal::windows::{ctrl_break, ctrl_c};

            let mut ctrl_c_stream = ctrl_c()?;
            let mut ctrl_break_stream = ctrl_break()?;

            loop {
                tokio::select! {
                    _ = ctrl_c_stream.recv() => {
                        info!("Received Ctrl+C, initiating graceful shutdown");
                        if self.shutdown_coordinator.initiate_shutdown(ShutdownReason::Signal(2)) {
                            break;
                        }
                    }
                    _ = ctrl_break_stream.recv() => {
                        info!("Received Ctrl+Break, initiating graceful shutdown");
                        if self.shutdown_coordinator.initiate_shutdown(ShutdownReason::Signal(3)) {
                            break;
                        }
                    }
                }

                if !self.handling_signals.load(Ordering::Acquire) {
                    break;
                }
            }
        }

        #[cfg(all(feature = "async-std", not(feature = "tokio")))]
        {
            while self.handling_signals.load(Ordering::Acquire) {
                async_std::task::sleep(std::time::Duration::from_millis(100)).await;
            }
        }

        Ok(())
    }

    /// Stop signal handling.
    pub fn stop(&self) {
        self.handling_signals.store(false, Ordering::Release);
        debug!("Configurable signal handling stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shutdown::ShutdownCoordinator;
    use std::time::Duration;

    #[test]
    fn test_signal_description() {
        assert_eq!(signal_description(15), "SIGTERM (Terminate)");
        assert_eq!(signal_description(2), "SIGINT (Interrupt/Ctrl+C)");
        assert_eq!(signal_description(999), "Unknown signal");
    }

    #[test]
    fn test_signal_config() {
        let config = SignalConfig::new()
            .with_sighup()
            .with_custom_handler(12, "Custom signal")
            .without_sigint();

        assert_eq!(config.interrupt, SignalHandling::Disabled);
        assert_eq!(config.term, SignalHandling::Enabled);
        assert_eq!(config.hangup, SignalHandling::Enabled);
        assert_eq!(config.custom_handlers.len(), 1);
        assert_eq!(config.custom_handlers[0].0, 12);
    }

    #[cfg(feature = "tokio")]
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_signal_handler_creation() {
        // Add a test timeout to prevent freezing
        let test_result = tokio::time::timeout(Duration::from_secs(5), async {
            let coordinator = ShutdownCoordinator::new(5000, 10000);
            let handler = SignalHandler::new(coordinator);

            assert!(!handler.is_handling());

            // Note: We can't easily test the actual signal handling without
            // sending real signals, which would be complex in a test environment
        })
        .await;

        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio")))]
    #[async_std::test]
    async fn test_signal_handler_creation() {
        // Add a test timeout to prevent freezing
        let test_result = async_std::future::timeout(Duration::from_secs(5), async {
            let coordinator = ShutdownCoordinator::new(5000, 10000);
            let handler = SignalHandler::new(coordinator);

            assert!(!handler.is_handling());

            // Note: We can't easily test the actual signal handling without
            // sending real signals, which would be complex in a test environment
        })
        .await;

        assert!(test_result.is_ok(), "Test timed out after 5 seconds");
    }
}
