//! Facility to wait for a dynamic set of tasks to complete, with a single
//! waiter and multiple waitees (things that are waited for). Notably, each
//! waitee can also start more work to be waited for.
//!
//! # Implementation Details
//!
//! The implementation of waiting in this module is just a wrapper around
//! [`tokio::sync::mpsc::channel`]. A [`WaitGroup`] holds the unique
//! [`tokio::sync::mpsc::Receiver`] and each [`WaitGuard`] holds a
//! [`tokio::sync::mpsc::Sender`]. Despite this simple implementation, the
//! [`WaitGroup`] and [`WaitGuard`] wrappers are useful to make this discoverable.

use std::convert::Infallible;
use tokio::sync::mpsc::{Receiver, Sender, channel};

/// Manager for a particular wait group. This can spawn a number of [`WaitGuard`]s
/// and it can then wait for them to all complete.
///
/// Each [`WaitGroup`] is single-use&mdash;calling [`WaitGroup::wait`] to start
/// waiting consumes the [`WaitGroup`]. Additionally, once all [`WaitGuard`]s
/// have been dropped, it is not possible to create any more [`WaitGuard`]s.
#[derive(Debug)]
pub struct WaitGroup {
    /// [`Receiver`] is held to wait for multiple [`Sender`]s and detect
    /// when they have closed. The [`Infallible`] type means no value can/will
    /// ever be received through the channel.
    recv: Receiver<Infallible>,
}

/// RAII guard held by a task which is being waited for.
///
/// The existence of values of this type represents outstanding work for
/// its corresponding [`WaitGroup`].
///
/// A [`WaitGuard`] can be cloned using [`WaitGuard::clone`]. This allows
/// a task to spawn additional tasks, recursively.
#[derive(Clone, Debug)]
pub struct WaitGuard {
    /// [`Sender`] is held to keep the [`Receiver`] end open (stored in [`WaitGroup`]).
    /// The dropping of all senders will cause the receiver to detect and close.
    /// The [`Infallible`] type means no value can/will ever be sent through the channel.
    _send: Sender<Infallible>,
}

impl WaitGroup {
    /// Creates a new [`WaitGroup`] and its first associated [`WaitGuard`].
    ///
    /// Note that [`WaitGroup`] itself has no ability to create new guards.
    /// If needed, new guards should be created by cloning the returned [`WaitGuard`].
    #[must_use]
    pub fn new() -> (Self, WaitGuard) {
        let (send, recv) = channel(1);
        (Self { recv }, WaitGuard { _send: send })
    }

    /// Waits, asynchronously, until all the associated [`WaitGuard`]s have finished.
    pub async fn wait(mut self) {
        let None = self.recv.recv().await;
    }
}
