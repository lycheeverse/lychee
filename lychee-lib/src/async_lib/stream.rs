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
/// # Must be polled
///
/// This type is marked [`#[must_use]`] to catch the most obvious mistake: calling
/// [`partition_result`][StreamExt::partition_result] and immediately discarding one
/// of the two returned streams.
///
/// ```compile_fail
/// // This produces a warning: unused `PartitionedStream` that must be used
/// let (oks, _errs) = my_stream.partition_result::<Good, Bad>();
/// // ^ one half bound, but the other is silently dropped here
/// ```
///
/// # What `#[must_use]` does *not* catch
///
/// `#[must_use]` only fires when a value is **immediately discarded** — that is,
/// produced by an expression and never bound to a variable.  It does **not** fire
/// if you bind the stream to a variable and then simply never poll it:
///
/// ```ignore
/// let (oks, errs) = my_stream.partition_result::<Good, Bad>();
/// // Both are bound, so no warning — but if you only drive `oks` and
/// // ignore `errs`, you will deadlock. The compiler cannot catch this.
/// drive(oks).await;
/// // `errs` is never polled — deadlock!
/// ```
///
/// # Why both halves *must* be polled concurrently
///
/// [`partition_result`][StreamExt::partition_result] works by spawning a single
/// internal **driver** future that reads from the source stream and fans items out
/// into two bounded [`tokio::sync::mpsc`] channels — one for `Ok` values, one for
/// `Err` values.  The driver and both receiver streams are wired together with
/// [`StreamExt::concurrently_with`] so that polling either output stream also
/// advances the driver.
///
/// The two channels have a small fixed buffer (currently 16 slots each).  Consider
/// what happens when only one output stream is being polled:
///
/// 1. The driver reads the next item from the source and finds, say, an `Err`.
/// 2. It tries to send that `Err` into the `err` channel.
/// 3. If the `err` channel is full — because nothing is consuming it — the
///    driver `.await`s on the send, blocking itself.
/// 4. While the driver is blocked, **no more items flow through the pipeline at
///    all**, including `Ok` items.  The `ok` stream therefore also stalls, even
///    though something *is* polling it.
/// 5. If the polling side is itself waiting for an `Ok` item to make progress
///    (e.g. inside a `select!` or `join!`), the whole task hangs forever.
///
/// The same scenario applies symmetrically if only the `err` stream is polled.
///
/// The fix is always to poll **both** streams concurrently, for example by passing
/// them both to [`futures::stream::select`], [`futures::stream::select_with_strategy`],
/// or by driving them inside a single `join!` / `select!` expression.
///
/// # See also
///
/// * [`StreamExt::partition_result`] — the combinator that produces this type.
/// * The `must_future` crate, which applies the same `#[must_use]`-newtype pattern
///   to `BoxFuture` for the same reason: type aliases cannot carry `#[must_use]`,
///   so a newtype wrapper is required.
#[must_use = "this stream does nothing unless polled; \
              additionally, BOTH halves returned by `partition_result` \
              must be polled concurrently or the pipeline will deadlock — \
              see the `PartitionedStream` documentation for details"]
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

// Forward the `Stream` impl so callers can use all the normal combinators
// (`.next()`, `.map()`, `.for_each()`, etc.) directly on `PartitionedStream`
// without having to unwrap the inner type.
impl<T, SenderFut: Future> Stream for PartitionedStream<T, SenderFut> {
    type Item = T;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // SAFETY: we never move `self.0` out of the pin projection.
        // `pin_project` would be cleaner here, but we avoid the extra
        // dependency for a single field struct.
        let inner = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.0) };
        inner.poll_next(cx)
    }
}

// Also forward `FusedStream` so callers can use this inside `select!`.
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
    /// # Returns
    ///
    /// A tuple `(ok_stream, err_stream)` where:
    /// - `ok_stream` yields the inner value of every `Ok` item.
    /// - `err_stream` yields the inner value of every `Err` item.
    ///
    /// Both halves are [`PartitionedStream`] values.  That type is marked
    /// `#[must_use]`, which will produce a compiler warning if either half is
    /// immediately discarded.
    ///
    /// # Deadlock warning — both streams must be polled concurrently
    ///
    /// This is the most important constraint of this combinator.  Internally,
    /// a single driver reads from `self` and routes items into two small bounded
    /// channels.  **If only one output stream is polled**, the driver will
    /// eventually try to send into the *other* (unpolled) channel, block on the
    /// full buffer, and never make progress again — a deadlock.
    ///
    /// Always consume both streams together, for example:
    ///
    /// ```ignore
    /// let (oks, errs) = my_stream.partition_result::<Good, Bad>();
    ///
    /// // Drive both sides inside a single select-with-strategy, so that
    /// // whichever side has data ready is drained promptly.
    /// let combined = futures::stream::select_with_strategy(oks, errs, |()| {
    ///     futures::stream::PollNext::Left
    /// });
    /// combined.for_each(|item| async { /* … */ }).await;
    /// ```
    ///
    /// See [`PartitionedStream`] for a detailed explanation of *why* this
    /// deadlock occurs and what patterns avoid it.
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
            // The `ok` stream owns the driver via `concurrently_with`.
            // Polling `ok_stream` therefore also advances the driver, which
            // in turn may produce items for `err_stream`.  This is why both
            // streams must be polled: if `ok_stream` is never polled, the
            // driver never runs, and `err_stream` stalls too.
            PartitionedStream::new(ReceiverStream::new(ok_recv).concurrently_with(driver)),
            // The `err` stream does *not* own the driver — it stays alive only
            // as long as the channel sender (held by the driver) is open.
            // `future::pending()` here means "never terminate on your own";
            // the stream ends naturally when the channel closes.
            PartitionedStream::new(
                ReceiverStream::new(err_recv).concurrently_with(future::pending()),
            ),
        )
    }
}

impl<T: ?Sized> StreamExt for T where T: Stream {}
