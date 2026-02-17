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
use std::convert::Infallible;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_stream::wrappers::UnboundedReceiverStream;

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

async fn quicksort_waiter_example<'a, T>(xs: &'a mut [T], waiter: WaitGroup, guard: WaitGuard)
where
    T: Ord + Copy + Send + 'static,
{
    let (send, recv) = unbounded_channel();

    let task = tokio::task::spawn(quicksort_waiter_example_task::<T>(
        recv,
        send.clone(),
        waiter,
    ));

    send.send((guard.clone(), xs)).expect("initial send");
    task.await;
}

async fn quicksort_waiter_example_task<'a, T: Ord>(
    recv: UnboundedReceiver<(WaitGuard, &'a mut [T])>,
    send: UnboundedSender<(WaitGuard, &'a mut [T])>,
    waiter: WaitGroup,
) {
    UnboundedReceiverStream::new(recv)
        .take_until(waiter.wait())
        .for_each(async |(guard, slice): (WaitGuard, &'a mut [T])| {
            if slice.is_empty() {
                return;
            }
            let (lower, higher) = partition(slice);
            send.send((guard.clone(), lower)).expect("send 1");
            send.send((guard.clone(), higher)).expect("send 1");
        })
        .await
}

fn partition<T: Ord>(slice: &mut [T]) -> (&mut [T], &mut [T]) {
    if let ([pivot], rest) = slice.split_at_mut(1) {
        let mut iter = rest.iter_mut();
        let mut current = iter.next();
        let mut num_lesser = 0usize;

        while let Some(x) = current {
            current = if *x < *pivot {
                num_lesser += 1;
                iter.next()
            } else {
                // `x` is big. we have to swap `x` to the back
                let Some(ref mut dest) = iter.next_back() else {
                    break;
                };
                std::mem::swap(*dest, x);
                Some(x)
            };
        }

        slice.swap(num_lesser, 0);
        slice.split_at_mut(num_lesser)
    } else {
        return (&mut [], &mut []);
    }
}

#[cfg(test)]
mod tests {

    use super::partition;
    use super::{WaitGroup, WaitGuard};

    // #[tokio::test]
    // async fn quicksort_basic() {
    //     let mut xs = [5, 4, 3, 2, 1];
    //     let (waiter, guard) = WaitGroup::new();
    //     quicksort_waiter_example(&mut xs, waiter, guard).await;
    // }
    #[test]
    fn test_partition() {
        let mut xs = [3, 100, 4, 5, 2, 1, 8, 1, 0, 6, 8];
        partition(&mut xs);
        println!("{:?}", xs);
    }
}
