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

use futures::StreamExt;
use futures::never::Never;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_stream::wrappers::{ReceiverStream, UnboundedReceiverStream};

/// Manager for a particular wait group. This can spawn a number of [`WaitGuard`]s
/// and it can then wait for them to all complete.
///
/// Each [`WaitGroup`] is single-use&mdash;calling [`WaitGroup::wait`] to start
/// waiting consumes the [`WaitGroup`]. Additionally, once all [`WaitGuard`]s
/// have been dropped, it is not possible to create any more [`WaitGuard`]s.
#[derive(Debug)]
pub struct WaitGroup {
    /// [`Receiver`] is held to wait for multiple [`Sender`]s and detect
    /// when they have closed. The [`Never`] type means no value can/will
    /// ever be received through the channel.
    recv: Receiver<Never>,
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
    /// The [`Never`] type means no value can/will ever be sent through the channel.
    _send: Sender<Never>,
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

/// Demonstrates use of the [`WaitGroup`] and [`WaitGuard`] to (very inefficiently)
/// compute the Fibonacci number `F(n)` using recursive channels.
///
/// The given `waiter` will be used to detect when the work has finished and it will
/// close the channels. Additionally, `waiter` can be omitted to show that without
/// the [`WaitGroup`], the tasks would not terminate.
#[allow(dead_code)]
async fn fibonacci_waiter_example(n: usize, waiter: Option<(WaitGroup, WaitGuard)>) -> usize {
    let (send, recv) = unbounded_channel();
    let (incr_count, recv_count) = channel(1);

    let (waiter, guard) = match waiter {
        Some((waiter, guard)) => (Some(waiter), Some(guard)),
        None => (None, None),
    };

    let recursive_task = tokio::task::spawn({
        let send = send.clone();
        fibonacci_waiter_example_task(recv, send, incr_count, waiter)
    });

    let count_task = tokio::task::spawn(async move {
        let count = ReceiverStream::new(recv_count).count();
        count.await
    });

    send.send((guard, n)).expect("initial send"); // note `guard` must be moved!

    let ((), result) = futures::try_join!(recursive_task, count_task).expect("join");
    result
}

/// An inefficient Fibonacci implementation. This computes `F(n)` by sending
/// by `n-1` and `n-2` back into the channel. This shows how one work item can
/// create multiple subsequent work items.
///
/// The result is determined by sending `()` into an increment channel and
/// reading the number of increments.
///
/// This is wildly inefficient because it does not cache any results. Computing
/// `F(n)` would generate `O(2^n)` channel items.
#[allow(dead_code)]
async fn fibonacci_waiter_example_task(
    recv: UnboundedReceiver<(Option<WaitGuard>, usize)>,
    send: UnboundedSender<(Option<WaitGuard>, usize)>,
    incr_count: Sender<()>,
    waiter: Option<WaitGroup>,
) {
    let stream = UnboundedReceiverStream::new(recv);
    let stream = match waiter {
        Some(waiter) => stream.take_until(waiter.wait()).left_stream(),
        None => stream.right_stream(),
    };

    stream
        .for_each(async |(guard, n)| match n {
            0 => (),
            1 => incr_count.send(()).await.expect("send incr"),
            n => {
                send.send((guard.clone(), n - 1)).expect("send 1");
                send.send((guard.clone(), n - 2)).expect("send 2");
            }
        })
        .await;
}

#[cfg(test)]
mod tests {
    use super::WaitGroup;
    use super::fibonacci_waiter_example;
    use std::time::Duration;

    fn timeout<F: IntoFuture>(fut: F) -> tokio::time::Timeout<F::IntoFuture> {
        tokio::time::timeout(Duration::from_millis(250), fut)
    }

    #[tokio::test]
    async fn fibonacci_basic_termination() {
        assert_eq!(fibonacci_waiter_example(0, Some(WaitGroup::new())).await, 0);
        assert_eq!(
            fibonacci_waiter_example(9, Some(WaitGroup::new())).await,
            34
        );
        assert_eq!(
            fibonacci_waiter_example(10, Some(WaitGroup::new())).await,
            55
        );
    }

    #[tokio::test]
    async fn fibonacci_nontermination_without_waiter() {
        // task does not terminate if WaitGroup is not used, due to recursive channels
        assert!(timeout(fibonacci_waiter_example(9, None)).await.is_err());

        // even a "trivial" case does not terminate.
        assert!(timeout(fibonacci_waiter_example(0, None)).await.is_err());
    }

    #[tokio::test]
    async fn fibonacci_nontermination_with_extra_guard() {
        // in these tests, we do use a WaitGroup but it doesn't terminate because we
        // *clone* the guard and the test function holds an extra guard, blocking
        // WaitGroup from returning. this is an example of something that can go wrong
        // when using the waiter.
        let (waiter, guard) = WaitGroup::new();
        assert!(
            timeout(fibonacci_waiter_example(9, Some((waiter, guard.clone()))))
                .await
                .is_err()
        );

        let (waiter, guard) = WaitGroup::new();
        assert!(
            timeout(fibonacci_waiter_example(0, Some((waiter, guard.clone()))))
                .await
                .is_err()
        );
    }
}
