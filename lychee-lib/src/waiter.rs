//! Facility to wait for a dynamic set of tasks to complete, with a single
//! waiter and multiple waitees (things that are waited for). Notably, each
//! waitee can also start more work to be waited for.

use std::sync::Arc;
use std::sync::Weak;
use tokio::sync::Notify;

/// Manager for a particular wait group. This can spawn a number of [`WaitGuard`]s
/// and it can then wait for them to all complete.
///
/// Each [`WaitGroup`] is single-use&mdash;once it finishes waiting, it
/// is consumed and cannot be restarted with new tasks.
#[derive(Debug)]
pub struct WaitGroup(Arc<Notify>);

/// Guard held by a task which is being waited for.
///
/// The existence of values of this type represents outstanding work for
/// its corresponding [`WaitGroup`].
#[derive(Debug)]
pub struct WaitGuard(Weak<Notify>);

impl Clone for WaitGuard {
    /// Clones the current [`WaitGuard`]. This is allows a task to spawn
    /// additional tasks, recursively.
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl Drop for WaitGuard {
    /// Drops the current [`WaitGuard`] and invokes the notifier to wake the
    /// [`WaitGroup`].
    fn drop(&mut self) {
        if let Some(notify) = self.0.upgrade() {
            notify.notify_one();
        }
    }
}

impl WaitGroup {
    /// Creates a new WaitGroup.
    pub fn new() -> Self {
        Self(Arc::new(Notify::new()))
    }

    /// Creates a guard to represent a started task.
    pub fn guard(&self) -> WaitGuard {
        WaitGuard(Arc::downgrade(&self.0))
    }

    /// Waits, asynchronously, until all the associated [`WaitGuard`]s have finished.
    pub async fn wait(mut self) {
        while let Err(x) = self.try_wait() {
            self = x;
            self.0.notified().await;
        }
    }

    /// Checks if the [`WaitGroup`] is finished at the current point in time.
    ///
    /// If so, returns `Ok(())` and consumes self. Otherwise, returns `Err(self)`
    /// back to the caller so they can repeat the check later.
    pub fn try_wait(mut self) -> Result<(), Self> {
        match Arc::get_mut(&mut self.0) {
            Some(_) => Ok(()),
            None => Err(self),
        }
    }
}
