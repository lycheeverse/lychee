//! Helper functions for async [`stream::Stream`] combinators and type aliases.

use futures::FutureExt as _;
use futures::StreamExt as _;
use futures::channel::mpsc::{self, Receiver};
use futures::{SinkExt, Stream};
use futures::{future, stream};

/// Stream returned by [`pending_until`].
pub type PendingUntil<T, Fut> = stream::TakeUntil<stream::Pending<T>, Fut>;

/// Stream returned by [`StreamExt::concurrently_with`].
pub type ConcurrentlyWith<St, Fut> = stream::Select<St, PendingUntil<<St as Stream>::Item, Fut>>;

/// Stream returned by [`StreamExt::partition_result`].
pub type Partitioned<T, Fut = future::Ready<()>> =
    stream::Select<Receiver<T>, PendingUntil<T, Fut>>;

/// A stream which is pending until the given future completes, at which point
/// the stream will terminate. The returned stream will never return any values,
/// so the choice of `T` can be arbitrary.
pub fn pending_until<T, Fut: Future>(fut: Fut) -> PendingUntil<T, Fut> {
    stream::pending().take_until(fut)
}

/// Useful stream combinators. See also [`futures::StreamExt`] ([online][]).
///
/// [online]: https://docs.rs/futures/latest/futures/stream/trait.StreamExt.html
pub trait StreamExt: Stream {
    /// A stream which wraps a stream while concurrently polling a given future.
    ///
    /// The future is only polled for its side effects. Its output is discarded.
    /// All items from the input stream will be propagated unchanged.
    ///
    /// However, the returned stream can only terminate after the future has
    /// completed (this is a shortcoming in the implementation).
    fn concurrently_with<Fut: Future>(self, fut: Fut) -> ConcurrentlyWith<Self, Fut>
    where
        Self: Sized,
    {
        stream::select(self, pending_until(fut))
    }

    /// Partitions the given stream of [`Result<T, E>`] into two streams&mdash;one yielding the
    /// `T` values and one yielding the `E` values.
    ///
    /// **Deadlocks**: Both returned streams must be polled concurrently to avoid deadlock!
    /// This combinator performs minimal buffering. If only one output stream is polled,
    /// encountering a result of the opposite type will block the stream.
    fn partition_result<T, E>(self) -> (Partitioned<T, impl Future>, Partitioned<E>)
    where
        Self: Stream<Item = Result<T, E>> + Sized,
    {
        let (ok_send, ok_recv) = mpsc::channel(1);
        let (err_send, err_recv) = mpsc::channel(1);

        let driver = self
            .map(move |x| (x, ok_send.clone(), err_send.clone()))
            .for_each(async |(x, mut ok_send, mut err_send)| match x {
                Ok(x) => ok_send.send(x).await.unwrap(),
                Err(x) => err_send.send(x).await.unwrap(),
            })
            .fuse();

        (
            ok_recv.concurrently_with(driver),
            err_recv.concurrently_with(future::ready(())),
        )
    }
}

impl<T: ?Sized> StreamExt for T where T: Stream {}
