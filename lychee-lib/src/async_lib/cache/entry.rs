//! Types which provide the getters and setters for the [`super::Cache`].

use std::sync::Arc;

use tokio::sync::watch::{Receiver, Sender, channel, error::RecvError};

/// A value returned on cache misses. The owner of this struct should compute
/// the value, then call [`CacheSetter::set`] to write the value into the cache.
///
/// Within a [`super::Cache`], exactly one [`CacheSetter`] is created per key.
/// If the [`CacheSetter`] is dropped without setting a value, its cache entry
/// will never be set and calls to a corresponding [`CacheGetter::wait`] will error.
#[derive(Debug)]
#[must_use]
pub struct CacheSetter<T>(Sender<Option<Arc<T>>>);

impl<T> CacheSetter<T> {
    /// Writes the given value into the cache, consuming this [`CacheSetter`] and
    /// returning a [`CacheGetter`] referencing the stored value.
    pub fn set(self, value: T) -> CacheGetter<T> {
        self.0.send_replace(Some(Arc::new(value)));
        self.subscribe()
    }

    /// Attaches and returns a new [`CacheGetter`] for this [`CacheSetter`].
    #[must_use]
    pub fn subscribe(&self) -> CacheGetter<T> {
        CacheGetter(self.0.subscribe())
    }

    /// Creates a new detached [`CacheSetter`]. That is, a setter which is
    /// not backed by any value within the cache.
    ///
    /// This can be useful to let uncacheable entities use the same cache-handling logic.
    pub fn new_detached() -> Self {
        Self(channel(None).0)
    }
}

/// A value returned on cache hits. [`CacheGetter::wait`] returns a future which
/// resolves when the cache value has been stored by the corresponding [`CacheSetter`].
#[derive(Debug)]
pub struct CacheGetter<T>(Receiver<Option<Arc<T>>>);

impl<T> CacheGetter<T> {
    /// Waits until the cache value is computed and stored by the corresponding
    /// [`CacheSetter`]. If the value has already been computed and stored, this
    /// function will complete immediately.
    ///
    /// # Errors
    /// Returns an error if the corresponding [`CacheSetter`] is dropped without
    /// setting a value.
    pub async fn wait(mut self) -> Result<Arc<T>, RecvError> {
        let received = self.0.wait_for(Option::is_some).await?;

        #[expect(clippy::missing_panics_doc, reason = "impossible due to is_some check")]
        Ok(received.as_ref().unwrap().clone())
    }

    /// Returns the value without waiting, if possible, otherwise returns [`None`].
    #[must_use]
    pub fn get(&self) -> Option<Arc<T>> {
        self.0.borrow().as_ref().cloned()
    }

    /// Returns the value without waiting, consuming this [`CacheGetter`] in the process.
    /// Returns [`None`] if the value is not ready.
    #[must_use]
    pub fn into_inner(self) -> Option<Arc<T>> {
        self.get()
    }

    /// Constructs a new [`CacheGetter`] which is immediately ready with the
    /// given value.
    pub fn ready(value: T) -> Self {
        let (_, recv) = channel(Some(Arc::new(value)));
        CacheGetter(recv)
    }
}

impl<T> Clone for CacheGetter<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
