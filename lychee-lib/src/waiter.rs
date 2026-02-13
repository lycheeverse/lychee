//! Facility to wait for a dynamic set of tasks to complete, with a single
//! waiter and multiple waitees (things that are waited for). Notably, each
//! waitee can also start more work to be waited for.

use tokio::sync::mpsc::{Receiver, Sender, channel};

/// Manager for a particular wait group. This can spawn a number of [`WaitGuard`]s
/// and it can then wait for them to all complete.
///
/// Each [`WaitGroup`] is single-use&mdash;once it finishes waiting, it
/// is consumed and cannot be restarted with new tasks.
#[derive(Debug)]
pub struct WaitGroup(Receiver<()>);

/// Guard held by a task which is being waited for.
///
/// The existence of values of this type represents outstanding work for
/// its corresponding [`WaitGroup`].
///
///
/// A [`WaitGuard`] can be cloned using [`WaitGuard::clone`]. This is allows
/// a task to spawn additional tasks, recursively.
#[derive(Clone, Debug)]
pub struct WaitGuard(
    #[allow(
        dead_code,
        reason = "Field is never accessed, but it is crucial to hold on to the value."
    )]
    Sender<()>,
);

impl WaitGroup {
    /// Creates a new WaitGroup.
    pub fn new() -> (Self, WaitGuard) {
        let (send, recv) = channel(1);
        (Self(recv), WaitGuard(send))
    }

    /// Waits, asynchronously, until all the associated [`WaitGuard`]s have finished.
    pub async fn wait(mut self) {
        self.0.recv().await;
    }
}
