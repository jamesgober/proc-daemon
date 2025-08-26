//! Lightweight coordination primitives with optional lock-free backend.
//!
//! When built with the `lockfree-coordination` feature, this module uses
//! `crossbeam-channel` for MPMC lock-free channels. Otherwise it falls back
//! to `std::sync::mpsc` unbounded channels.

#[cfg(feature = "lockfree-coordination")]
/// Channel facade backed by `crossbeam-channel` when the
/// `lockfree-coordination` feature is enabled.
pub mod chan {
    pub use crossbeam_channel::{unbounded, Receiver, Sender, TryRecvError};

    /// Non-blocking receive helper that mirrors the fallback API.
    ///
    /// Returns the next message if available or a `TryRecvError` if the
    /// channel is empty or disconnected.
    ///
    /// # Errors
    ///
    /// Returns `Err(TryRecvError)` when the channel is empty or disconnected.
    #[inline]
    pub fn try_recv<T>(rx: &Receiver<T>) -> Result<T, TryRecvError> {
        rx.try_recv()
    }
}

#[cfg(not(feature = "lockfree-coordination"))]
/// Channel facade backed by `std::sync::mpsc` when the
/// `lockfree-coordination` feature is disabled.
pub mod chan {
    use std::sync::mpsc;

    /// Type alias for `mpsc::Sender`.
    pub type Sender<T> = mpsc::Sender<T>;

    /// Type alias for `mpsc::Receiver`.
    pub type Receiver<T> = mpsc::Receiver<T>;

    /// Error type for non-blocking receive operations.
    #[derive(Debug)]
    pub enum TryRecvError {
        /// The channel is empty.
        Empty,
        /// The channel is disconnected.
        Disconnected,
    }

    /// Create an unbounded channel returning `(Sender, Receiver)`.
    #[must_use]
    pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
        mpsc::channel()
    }

    /// Non-blocking receive helper that mirrors the crossbeam API.
    ///
    /// Returns the next message if available or a `TryRecvError` if the
    /// channel is empty or disconnected.
    ///
    /// # Errors
    ///
    /// Returns `Err(TryRecvError)` when the channel is empty or disconnected.
    #[inline]
    pub fn try_recv<T>(rx: &Receiver<T>) -> Result<T, TryRecvError> {
        rx.try_recv().map_err(|e| match e {
            mpsc::TryRecvError::Empty => TryRecvError::Empty,
            mpsc::TryRecvError::Disconnected => TryRecvError::Disconnected,
        })
    }
}
