//! Error handling for the proc-daemon framework.
//!
//! This module provides comprehensive error types for all daemon operations,
//! designed for both programmatic handling and human-readable error messages.
//!
//! # Features
//!
//! * Structured error codes for all error types
//! * Source error capture for better context
//! * Backtrace support for production debugging
//! * Serializable errors for structured logging (requires `serde` feature)
//! * Categorization for metrics and monitoring
//!
//! # Error Structure
//!
//! Each error variant contains:
//!
//! * **Error Code**: A unique identifier for programmatic handling and metrics
//! * **Message**: A human-readable description of the error
//! * **Source**: Optional underlying error that caused this error
//! * **Context-specific fields**: Additional fields specific to each error type
//!
//! # Usage Examples
//!
//! ## Basic Error Handling
//!
//! ```no_run
//! use proc_daemon::{Error, Result};
//!
//! fn example_function() -> Result<()> {
//!     // Use prebuilt constructor
//!     let something_failed = true;
//!     if something_failed {
//!         return Err(Error::config("Invalid configuration"));
//!     }
//!
//!     // Or with specific error code
//!     let config_missing = false;
//!     if config_missing {
//!         // Code using specific error codes would go here
//!         return Err(Error::config("Configuration file not found"));
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Capturing Source Errors
//!
//! ```no_run
//! use proc_daemon::{Error, Result};
//! use std::fs::File;
//!
//! fn read_config(path: &str) -> Result<String> {
//!     let file = match File::open(path) {
//!         Ok(f) => f,
//!         Err(err) => return Err(Error::io_with_source(
//!             format!("Failed to open config file: {}", path),
//!             err
//!         )),
//!     };
//!     
//!     // Process file...
//!     Ok("Config content".to_string())
//! }
//! ```
//!
//! ## Error Handling Best Practices
//!
//! 1. Always include meaningful error messages
//! 2. Capture source errors when available
//! 3. Use specific error codes for monitoring and metrics
//! 4. Enable backtraces in development and test environments
//!
//! # Feature Flags
//!
//! ## Backtrace Support
//!
//! Enable backtrace support with the `backtrace` feature:
//!
//! ```toml
//! [dependencies]
//! proc-daemon = { version = "0.1.0", features = ["backtrace"] }
//! ```
//!
//! ## Serialization
//!
//! Enable error serialization with the `serde` feature:
//!
//! ```toml
//! [dependencies]
//! proc-daemon = { version = "0.1.0", features = ["serde"] }
//! ```
//!
//! This allows errors to be serialized for structured logging or metrics collection.

/// Result type alias for proc-daemon operations.
pub type Result<T> = std::result::Result<T, Error>;

// BacktraceError implementation for capturing backtraces
#[cfg(feature = "backtrace")]
pub use std::backtrace::Backtrace;

/// Error type that captures backtraces
#[cfg(feature = "backtrace")]
#[derive(Debug)]
pub struct BacktraceError {
    /// The message describing this error
    message: String,
    /// The backtrace captured when this error was created
    backtrace: Backtrace,
    /// Optional source error
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

#[cfg(feature = "backtrace")]
impl BacktraceError {
    /// Create a new backtrace error with a message
    #[allow(dead_code)]
    pub fn new<S: Into<String>>(message: S) -> Self {
        Self {
            message: message.into(),
            backtrace: Backtrace::capture(),
            source: None,
        }
    }

    /// Create a new backtrace error with a message and source
    #[allow(dead_code)]
    pub fn with_source<S: Into<String>, E: std::error::Error + Send + Sync + 'static>(
        message: S,
        source: E,
    ) -> Self {
        Self {
            message: message.into(),
            backtrace: Backtrace::capture(),
            source: Some(Box::new(source)),
        }
    }

    /// Get the backtrace
    pub const fn backtrace(&self) -> &Backtrace {
        &self.backtrace
    }
}

#[cfg(feature = "backtrace")]
impl std::fmt::Display for BacktraceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

#[cfg(feature = "backtrace")]
impl std::error::Error for BacktraceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|s| s.as_ref() as &(dyn std::error::Error + 'static))
    }
}

/// Error code enum for categorizing and identifying errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum ErrorCode {
    // Configuration errors: 1000-1999
    ConfigInvalid = 1000,
    ConfigParse = 1001,
    ConfigMissing = 1002,
    ConfigTypeMismatch = 1003,

    // Signal handling errors: 2000-2999
    SignalRegisterFailed = 2000,
    SignalSendFailed = 2001,
    SignalInvalid = 2002,

    // Shutdown errors: 3000-3999
    ShutdownTimeout = 3000,
    ShutdownAlreadyInProgress = 3001,
    ShutdownFailed = 3002,

    // Subsystem errors: 4000-4999
    SubsystemStartFailed = 4000,
    SubsystemStopFailed = 4001,
    SubsystemNotFound = 4002,
    SubsystemAlreadyRegistered = 4003,
    SubsystemStateInvalid = 4004,

    // IO errors: 5000-5999
    IoError = 5000,
    FileNotFound = 5001,
    FilePermissionDenied = 5002,

    // Runtime errors: 6000-6999
    RuntimePanic = 6000,
    RuntimeAsyncError = 6001,
    LockFailed = 6002,
    RuntimeSpawnError = 6003,
    MissingRuntime = 6004,

    // Resource errors: 7000-7999
    ResourceExhaustedMemory = 7000,
    ResourceExhaustedCpu = 7001,
    ResourceExhaustedFileDescriptors = 7002,

    // Timeout errors: 8000-8999
    TimeoutOperation = 8000,
    TimeoutConnection = 8001,

    // State errors: 9000-9999
    InvalidStateTransition = 9000,
    InvalidStateValue = 9001,

    // Platform errors: 10000-10999
    PlatformNotSupported = 10000,
    PlatformFeatureNotAvailable = 10001,

    // Unknown/other errors: 99000+
    Unknown = 99999,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}({})", self.as_str(), *self as i32)
    }
}

impl ErrorCode {
    /// Convert error code to string representation
    pub const fn as_str(self) -> &'static str {
        match self {
            // Configuration errors
            Self::ConfigInvalid => "CONFIG_INVALID",
            Self::ConfigParse => "CONFIG_PARSE",
            Self::ConfigMissing => "CONFIG_MISSING",
            Self::ConfigTypeMismatch => "CONFIG_TYPE_MISMATCH",

            // Signal errors
            Self::SignalRegisterFailed => "SIGNAL_REGISTER_FAILED",
            Self::SignalSendFailed => "SIGNAL_SEND_FAILED",
            Self::SignalInvalid => "SIGNAL_INVALID",

            // Shutdown errors
            Self::ShutdownTimeout => "SHUTDOWN_TIMEOUT",
            Self::ShutdownAlreadyInProgress => "SHUTDOWN_ALREADY_IN_PROGRESS",
            Self::ShutdownFailed => "SHUTDOWN_FAILED",

            // Subsystem errors
            Self::SubsystemStartFailed => "SUBSYSTEM_START_FAILED",
            Self::SubsystemStopFailed => "SUBSYSTEM_STOP_FAILED",
            Self::SubsystemNotFound => "SUBSYSTEM_NOT_FOUND",
            Self::SubsystemAlreadyRegistered => "SUBSYSTEM_ALREADY_REGISTERED",
            Self::SubsystemStateInvalid => "SUBSYSTEM_STATE_INVALID",

            // IO errors
            Self::IoError => "IO_ERROR",
            Self::FileNotFound => "FILE_NOT_FOUND",
            Self::FilePermissionDenied => "FILE_PERMISSION_DENIED",

            // Runtime errors
            Self::RuntimePanic => "RUNTIME_PANIC",
            Self::RuntimeAsyncError => "RUNTIME_ASYNC_ERROR",
            Self::LockFailed => "LOCK_FAILED",
            Self::RuntimeSpawnError => "RUNTIME_SPAWN_ERROR",
            Self::MissingRuntime => "MISSING_RUNTIME",

            // Resource errors
            Self::ResourceExhaustedMemory => "RESOURCE_EXHAUSTED_MEMORY",
            Self::ResourceExhaustedCpu => "RESOURCE_EXHAUSTED_CPU",
            Self::ResourceExhaustedFileDescriptors => "RESOURCE_EXHAUSTED_FILE_DESCRIPTORS",

            // Timeout errors
            Self::TimeoutOperation => "TIMEOUT_OPERATION",
            Self::TimeoutConnection => "TIMEOUT_CONNECTION",

            // State errors
            Self::InvalidStateTransition => "INVALID_STATE_TRANSITION",
            Self::InvalidStateValue => "INVALID_STATE_VALUE",

            // Platform errors
            Self::PlatformNotSupported => "PLATFORM_NOT_SUPPORTED",
            Self::PlatformFeatureNotAvailable => "PLATFORM_FEATURE_NOT_AVAILABLE",

            // Unknown errors
            Self::Unknown => "UNKNOWN_ERROR",
        }
    }
}

/// Comprehensive error type for all daemon operations.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Configuration-related errors
    #[error("Configuration error [{code}]: {message}")]
    Config {
        /// Error code for structured error handling
        code: ErrorCode,
        /// Human-readable error message
        message: String,
        /// Optional source error for better context
        #[source]
        #[cfg_attr(feature = "serde", serde(skip))]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    /// Signal handling errors
    #[error("Signal handling error [{code}]: {message}{signal:?}")]
    Signal {
        /// Error code for structured error handling
        code: ErrorCode,
        /// Human-readable error message
        message: String,
        /// Signal number if applicable
        signal: Option<i32>,
        /// Optional source error for better context
        #[source]
        #[cfg_attr(feature = "serde", serde(skip))]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    /// Shutdown coordination errors
    #[error("Shutdown error [{code}]: {message}{timeout_ms:?}")]
    Shutdown {
        /// Error code for structured error handling
        code: ErrorCode,
        /// Human-readable error message
        message: String,
        /// Shutdown timeout if applicable
        timeout_ms: Option<u64>,
        /// Optional source error for better context
        #[source]
        #[cfg_attr(feature = "serde", serde(skip))]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    /// Subsystem management errors
    #[error("Subsystem '{name}' error [{code}]: {message}")]
    Subsystem {
        /// Error code for structured error handling
        code: ErrorCode,
        /// Name of the subsystem
        name: String,
        /// Human-readable error message
        message: String,
        /// Optional source error for better context
        #[source]
        #[cfg_attr(feature = "serde", serde(skip))]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    /// I/O operation errors
    #[error("I/O error [{code}]: {message}")]
    Io {
        /// Error code for structured error handling
        code: ErrorCode,
        /// Human-readable error message
        message: String,
        /// Optional source error for better context
        #[source]
        #[cfg_attr(feature = "serde", serde(skip))]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    /// Resource exhaustion errors
    #[error("Resource exhausted [{code}]: {resource} - {message}")]
    ResourceExhausted {
        /// Error code for structured error handling
        code: ErrorCode,
        /// Type of resource exhausted
        resource: String,
        /// Human-readable error message
        message: String,
        /// Optional source error for better context
        #[source]
        #[cfg_attr(feature = "serde", serde(skip))]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    /// Timeout errors
    #[error("Operation timed out [{code}] after {timeout_ms}ms: {operation}")]
    Timeout {
        /// Error code for structured error handling
        code: ErrorCode,
        /// Operation that timed out
        operation: String,
        /// Timeout duration in milliseconds
        timeout_ms: u64,
        /// Optional source error for better context
        #[source]
        #[cfg_attr(feature = "serde", serde(skip))]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    /// Invalid state errors
    #[error("Invalid state [{code}]: {message}{current_state:?}")]
    InvalidState {
        /// Error code for structured error handling
        code: ErrorCode,
        /// Human-readable error message
        message: String,
        /// Current state if applicable
        current_state: Option<String>,
        /// Optional source error for better context
        #[source]
        #[cfg_attr(feature = "serde", serde(skip))]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    /// Platform-specific errors
    #[error("Platform error [{code}]: {message} (platform: {platform})")]
    Platform {
        /// Error code for structured error handling
        code: ErrorCode,
        /// Human-readable error message
        message: String,
        /// Platform identifier
        platform: String,
        /// Optional source error for better context
        #[source]
        #[cfg_attr(feature = "serde", serde(skip))]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    /// Runtime errors
    #[error("Runtime error [{code}]: {message}")]
    Runtime {
        /// Error code for structured error handling
        code: ErrorCode,
        /// Human-readable error message
        message: String,
        /// Optional source error for better context
        #[source]
        #[cfg_attr(feature = "serde", serde(skip))]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },
}

impl Error {
    // Helper methods removed as they're no longer needed with direct field formatting

    #[cfg(feature = "backtrace")]
    /// Get a backtrace for the error if available
    #[must_use]
    pub fn backtrace(&self) -> Option<&Backtrace> {
        match self {
            Self::Config {
                source: Some(err), ..
            }
            | Self::Subsystem {
                source: Some(err), ..
            }
            | Self::Io {
                source: Some(err), ..
            }
            | Self::Runtime {
                source: Some(err), ..
            }
            | Self::ResourceExhausted {
                source: Some(err), ..
            }
            | Self::Timeout {
                source: Some(err), ..
            }
            | Self::InvalidState {
                source: Some(err), ..
            }
            | Self::Platform {
                source: Some(err), ..
            }
            | Self::Signal {
                source: Some(err), ..
            }
            | Self::Shutdown {
                source: Some(err), ..
            } => err
                .downcast_ref::<BacktraceError>()
                .map(BacktraceError::backtrace),
            _ => None,
        }
    }

    /// Create a new configuration error.
    pub fn config<S: Into<String>>(message: S) -> Self {
        Self::Config {
            code: ErrorCode::ConfigInvalid,
            message: message.into(),
            source: None,
        }
    }

    /// Create a new signal error.
    pub fn signal<S: Into<String>>(message: S) -> Self {
        Self::Signal {
            code: ErrorCode::SignalInvalid,
            message: message.into(),
            signal: None,
            source: None,
        }
    }

    /// Create a new signal error with signal number.
    pub fn signal_with_number<S: Into<String>>(message: S, signal: i32) -> Self {
        Self::Signal {
            code: ErrorCode::SignalInvalid,
            message: message.into(),
            signal: Some(signal),
            source: None,
        }
    }

    /// Create a new signal error with specific code.
    pub fn signal_with_code<S: Into<String>>(code: ErrorCode, message: S) -> Self {
        Self::Signal {
            code,
            message: message.into(),
            signal: None,
            source: None,
        }
    }

    /// Create a new shutdown error.
    pub fn shutdown<S: Into<String>>(message: S) -> Self {
        Self::Shutdown {
            code: ErrorCode::ShutdownFailed,
            message: message.into(),
            timeout_ms: None,
            source: None,
        }
    }

    /// Create a new shutdown error with timeout.
    pub fn shutdown_timeout<S: Into<String>>(message: S, timeout_ms: u64) -> Self {
        Self::Shutdown {
            code: ErrorCode::ShutdownTimeout,
            message: message.into(),
            timeout_ms: Some(timeout_ms),
            source: None,
        }
    }

    /// Create a new shutdown error with specific error code.
    pub fn shutdown_with_code<S: Into<String>>(code: ErrorCode, message: S) -> Self {
        Self::Shutdown {
            code,
            message: message.into(),
            timeout_ms: None,
            source: None,
        }
    }

    /// Create a new subsystem error.
    pub fn subsystem<S: Into<String>, M: Into<String>>(name: S, message: M) -> Self {
        Self::Subsystem {
            code: ErrorCode::SubsystemNotFound,
            name: name.into(),
            message: message.into(),
            source: None,
        }
    }

    /// Create a new subsystem error with specific error code.
    pub fn subsystem_with_code<S: Into<String>, M: Into<String>>(
        code: ErrorCode,
        name: S,
        message: M,
    ) -> Self {
        Self::Subsystem {
            code,
            name: name.into(),
            message: message.into(),
            source: None,
        }
    }

    /// Create a new I/O error.
    pub fn io<S: Into<String>>(message: S) -> Self {
        Self::Io {
            code: ErrorCode::IoError,
            message: message.into(),
            source: None,
        }
    }

    /// Create a new I/O error with source error.
    pub fn io_with_source<S: Into<String>, E: std::error::Error + Send + Sync + 'static>(
        message: S,
        source: E,
    ) -> Self {
        Self::Io {
            code: ErrorCode::IoError,
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Create a new runtime error.
    pub fn runtime<S: Into<String>>(message: S) -> Self {
        Self::Runtime {
            code: ErrorCode::RuntimePanic,
            message: message.into(),
            source: None,
        }
    }

    /// Create a new runtime error with specific code.
    pub fn runtime_with_code<S: Into<String>>(code: ErrorCode, message: S) -> Self {
        Self::Runtime {
            code,
            message: message.into(),
            source: None,
        }
    }

    /// Create a new runtime error with source error.
    pub fn runtime_with_source<S: Into<String>, E: std::error::Error + Send + Sync + 'static>(
        message: S,
        source: E,
    ) -> Self {
        Self::Runtime {
            code: ErrorCode::RuntimePanic,
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Create a new resource exhausted error.
    pub fn resource_exhausted<S: Into<String>, M: Into<String>>(resource: S, message: M) -> Self {
        Self::ResourceExhausted {
            code: ErrorCode::ResourceExhaustedMemory,
            resource: resource.into(),
            message: message.into(),
            source: None,
        }
    }

    /// Create a new resource exhausted error with specific code.
    pub fn resource_exhausted_with_code<S: Into<String>, M: Into<String>>(
        code: ErrorCode,
        resource: S,
        message: M,
    ) -> Self {
        Self::ResourceExhausted {
            code,
            resource: resource.into(),
            message: message.into(),
            source: None,
        }
    }

    /// Create a new timeout error.
    pub fn timeout<S: Into<String>>(operation: S, timeout_ms: u64) -> Self {
        Self::Timeout {
            code: ErrorCode::TimeoutOperation,
            operation: operation.into(),
            timeout_ms,
            source: None,
        }
    }

    /// Create a new timeout error with specific code and source.
    pub fn timeout_with_source<S: Into<String>, E: std::error::Error + Send + Sync + 'static>(
        operation: S,
        timeout_ms: u64,
        source: E,
    ) -> Self {
        Self::Timeout {
            code: ErrorCode::TimeoutOperation,
            operation: operation.into(),
            timeout_ms,
            source: Some(Box::new(source)),
        }
    }

    /// Create a new invalid state error.
    pub fn invalid_state<S: Into<String>>(message: S) -> Self {
        Self::InvalidState {
            code: ErrorCode::InvalidStateValue,
            message: message.into(),
            current_state: None,
            source: None,
        }
    }

    /// Create a new invalid state error with current state.
    pub fn invalid_state_with_current<S: Into<String>, C: Into<String>>(
        message: S,
        current_state: C,
    ) -> Self {
        Self::InvalidState {
            code: ErrorCode::InvalidStateTransition,
            message: message.into(),
            current_state: Some(current_state.into()),
            source: None,
        }
    }

    /// Create a new invalid state error with specific code.
    pub fn invalid_state_with_code<S: Into<String>>(code: ErrorCode, message: S) -> Self {
        Self::InvalidState {
            code,
            message: message.into(),
            current_state: None,
            source: None,
        }
    }

    /// Create a new platform error.
    pub fn platform<S: Into<String>, P: Into<String>>(message: S, platform: P) -> Self {
        Self::Platform {
            code: ErrorCode::PlatformNotSupported,
            message: message.into(),
            platform: platform.into(),
            source: None,
        }
    }

    /// Create a new platform error with specific code.
    pub fn platform_with_code<S: Into<String>, P: Into<String>>(
        code: ErrorCode,
        message: S,
        platform: P,
    ) -> Self {
        Self::Platform {
            code,
            message: message.into(),
            platform: platform.into(),
            source: None,
        }
    }

    /// Check if this error is retryable.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Io { .. } | Self::Runtime { .. } | Self::ResourceExhausted { .. }
        )
    }

    /// Check if this error is a timeout.
    #[must_use]
    pub const fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout { .. })
    }

    /// Check if this error is configuration-related.
    #[must_use]
    pub const fn is_config_error(&self) -> bool {
        matches!(self, Self::Config { .. })
    }

    /// Get the error category for metrics/logging.
    #[must_use]
    pub const fn category(&self) -> &'static str {
        match self {
            Self::Config { .. } => "config",
            Self::Signal { .. } => "signal",
            Self::Shutdown { .. } => "shutdown",
            Self::Subsystem { .. } => "subsystem",
            Self::Io { .. } => "io",
            Self::Runtime { .. } => "runtime",
            Self::ResourceExhausted { .. } => "resource",
            Self::Timeout { .. } => "timeout",
            Self::InvalidState { .. } => "state",
            Self::Platform { .. } => "platform",
        }
    }
}

// Implement From for common error types
impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::io(format!("I/O operation failed: {err}"))
    }
}

impl From<ctrlc::Error> for Error {
    fn from(err: ctrlc::Error) -> Self {
        Self::signal(format!("Signal handler error: {}", err))
    }
}

impl From<figment::Error> for Error {
    fn from(err: figment::Error) -> Self {
        Self::config(format!("Configuration loading failed: {err}"))
    }
}

#[cfg(feature = "toml")]
impl From<toml::de::Error> for Error {
    fn from(err: toml::de::Error) -> Self {
        Self::config(format!("TOML parsing failed: {err}"))
    }
}

#[cfg(feature = "serde_json")]
impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::config(format!("JSON parsing failed: {err}"))
    }
}

/// Helper macro for creating errors with formatted messages.
#[macro_export]
macro_rules! daemon_error {
    ($kind:ident, $($arg:tt)*) => {
        $crate::Error::$kind(format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = Error::config("test message");
        assert!(err.is_config_error());
        assert_eq!(err.category(), "config");
    }

    #[test]
    fn test_retryable_errors() {
        let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");
        let err = Error::io(format!("operation failed: {io_err}"));
        assert!(err.is_retryable());
    }

    #[test]
    fn test_timeout_error() {
        let err = Error::timeout("test operation", 5000);
        assert!(err.is_timeout());
        assert_eq!(err.category(), "timeout");
    }
}
