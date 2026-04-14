//! Helper functions for async [`stream::Stream`] combinators and type aliases.

use futures::FutureExt as _;
use futures::Stream;
use futures::StreamExt as _;
use futures::future::FusedFuture;
use futures::never::Never;
use futures::{future, stream};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

/// Stream returned by [`pending_until`].
pub type PendingUntil<T, Fut> = stream::TakeUntil<stream::Pending<T>, Fut>;

/// Stream returned by [`StreamExt::concurrently_with`].
pub type ConcurrentlyWith<St, Fut> =
    stream::TakeUntil<St, future::Join<Fut, future::Pending<Never>>>;

/// One half of the pair returned by [`StreamExt::partition_result`].
///
/// This type is marked `#[must_use]` to catch the most obvious mistake: calling
/// [`partition_result`][StreamExt::partition_result] and immediately discarding one
/// of the returned streams without driving it.
///
/// Note that `#[must_use]` only fires when a value is immediately discarded —
/// it does not fire if you bind the stream to a variable and then never poll it.
/// The harder mistake therefore looks like this:
///
/// ```ignore
/// let (oks, errs) = my_stream.partition_result::<Good, Bad>();
/// drive(oks).await;
/// // `errs` is never polled — deadlock, no compiler warning
/// ```
///
/// Both halves must be polled concurrently because they share a single internal
/// driver that reads from the source stream and fans items into two bounded
/// channels. If only one half is polled, the driver will eventually block trying
/// to send into the full, unread channel, and the whole pipeline stalls — including
/// the half that is being polled.
///
/// See [`StreamExt::partition_result`] for usage examples.
///
/// This pattern — a `#[must_use]` newtype wrapping a type alias — is also used by
/// the `must_future` crate, since type aliases cannot carry `#[must_use]` directly.
#[must_use = "streams do nothing unless polled; both halves returned by \
              `partition_result` must be polled concurrently or the pipeline \
              will deadlock — see the `PartitionedStream` docs for details"]
pub struct PartitionedStream<T, SenderFut = future::Pending<Never>>(
    stream::TakeUntil<ReceiverStream<T>, future::Join<SenderFut, future::Pending<Never>>>,
)
where
    SenderFut: Future;

impl<T, SenderFut: Future> PartitionedStream<T, SenderFut> {
    fn new(
        inner: stream::TakeUntil<
            ReceiverStream<T>,
            future::Join<SenderFut, future::Pending<Never>>,
        >,
    ) -> Self {
        Self(inner)
    }
}

// forward the `Stream` impl so callers can use combinators directly on
// `PartitionedStream` without unwrapping the inner type.
impl<T, SenderFut: Future> Stream for PartitionedStream<T, SenderFut> {
    type Item = T;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // SAFETY: we never move `self.0` out of the pin projection.
        // `pin_project` would be cleaner but adds a dependency for a single field.
        let inner = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.0) };
        inner.poll_next(cx)
    }
}

// forward `FusedStream` so this can be used inside `select!`.
impl<T, SenderFut: Future> futures::stream::FusedStream for PartitionedStream<T, SenderFut> {
    fn is_terminated(&self) -> bool {
        self.0.is_terminated()
    }
}

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

    /// Partitions the given stream of [`Result<T, E>`] into two streams — one
    /// yielding the `T` values and one yielding the `E` values.
    ///
    /// Both returned [`PartitionedStream`] halves must be polled concurrently.
    /// Internally a single driver reads from `self` and routes items into two
    /// bounded channels. If either half is left unpolled its channel fills up,
    /// the driver blocks, and the other half stalls too — a deadlock.
    ///
    /// The typical way to avoid this is to merge both halves back together with
    /// [`futures::stream::select`] or [`futures::stream::select_with_strategy`]
    /// and drive the combined stream:
    ///
    /// ```ignore
    /// let (oks, errs) = my_stream.partition_result::<Good, Bad>();
    /// let combined = futures::stream::select(oks, errs);
    /// combined.for_each(|item| async { /* … */ }).await;
    /// ```
    fn partition_result<T, E>(
        self,
    ) -> (
        PartitionedStream<T, impl FusedFuture<Output = ()>>,
        PartitionedStream<E>,
    )
    where
        Self: Stream<Item = Result<T, E>> + Sized,
    {
        let (ok_send, ok_recv) = mpsc::channel(16);
        let (err_send, err_recv) = mpsc::channel(16);

        let driver = self
            .map(move |x| (x, ok_send.clone(), err_send.clone()))
            .for_each(async |(x, ok_send, err_send)| match x {
                Ok(x) => ok_send.send(x).await.unwrap(),
                Err(x) => err_send.send(x).await.unwrap(),
            })
            .fuse();

        (
            // the `ok` stream owns the driver via `concurrently_with`, so
            // polling it also advances the driver and unblocks the `err` side.
            PartitionedStream::new(ReceiverStream::new(ok_recv).concurrently_with(driver)),
            // the `err` stream doesn't own the driver — it terminates naturally
            // when the driver closes the channel sender on completion.
            PartitionedStream::new(
                ReceiverStream::new(err_recv).concurrently_with(future::pending()),
            ),
        )
    }
}

impl<T: ?Sized> StreamExt for T where T: Stream {}
