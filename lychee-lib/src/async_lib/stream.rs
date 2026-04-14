//! Helper functions for async [`stream::Stream`] combinators and type aliases.

use futures::FutureExt as _;
use futures::Stream;
use futures::StreamExt as _;
use futures::future::FusedFuture;
use futures::never::Never;
use futures::{future, stream};
use log::warn;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

/// Number of elements to store in channels which are *essentially* unbuffered.
///
/// This is greater than 1 because Tokio allocates channel elements in blocks
/// of size 16 or 32, so this uses no extra memory and gives the Tokio runtime
/// the option to do multiple enqueue operations in a row, if it decides this
/// is beneficial.
///
/// Users of these channels should *not* rely on any buffering behaviour.
const TOKIO_SMALL_CHANNEL_SIZE: usize = 16;

/// Stream returned by [`pending_until`].
pub type PendingUntil<T, Fut> = stream::TakeUntil<stream::Pending<T>, Fut>;

/// Stream returned by [`StreamExt::concurrently_with`].
pub type ConcurrentlyWith<St, Fut> =
    stream::TakeUntil<St, future::Join<Fut, future::Pending<Never>>>;

/// Stream returned by [`StreamExt::partition_result`].
pub type Partitioned<T, SenderFut = future::Pending<Never>> =
    stream::TakeUntil<ReceiverStream<T>, future::Join<SenderFut, future::Pending<Never>>>;

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
    /// All items from the input stream will be propagated unchanged. The returned
    /// stream will terminate when the argument stream terminates.
    fn concurrently_with<Fut: Future>(self, fut: Fut) -> ConcurrentlyWith<Self, Fut>
    where
        Self: Sized,
    {
        self.take_until(future::join(fut, future::pending()))
    }

    /// Partitions the given stream of [`Result<T, E>`] into two streams&mdash;one yielding the
    /// `T` values and one yielding the `E` values.
    ///
    /// **Deadlocks**: Both returned streams must be polled concurrently to avoid deadlock!
    /// This combinator performs minimal buffering. If only one output stream is polled,
    /// encountering a result of the opposite type will block the stream.
    #[must_use = "partitioned streams must be polled, and both streams should be polled concurrently"]
    fn partition_result<T, E>(
        self,
    ) -> (
        Partitioned<T, impl FusedFuture<Output = ()>>,
        Partitioned<E>,
    )
    where
        Self: Stream<Item = Result<T, E>> + Sized,
    {
        let (ok_send, ok_recv) = mpsc::channel(TOKIO_SMALL_CHANNEL_SIZE);
        let (err_send, err_recv) = mpsc::channel(TOKIO_SMALL_CHANNEL_SIZE);

        let driver = self
            .map(move |x| (x, ok_send.clone(), err_send.clone()))
            .for_each(async |(x, ok_send, err_send)| match x {
                Ok(x) => ok_send.send(x).await.unwrap_or_else(|_| {
                    warn!("partition_result: cannot send item. Ok channel has been closed")
                }),
                Err(x) => err_send.send(x).await.unwrap_or_else(|_| {
                    warn!("partition_result: cannot send item. Err channel has been closed")
                }),
            })
            .fuse();
        // When finished, `.fuse()` drops the closure which owns the channel senders.
        // This is important for termination.

        (
            ReceiverStream::new(ok_recv).concurrently_with(driver),
            ReceiverStream::new(err_recv).concurrently_with(future::pending()),
        )
    }
}

impl<T: ?Sized> StreamExt for T where T: Stream {}
