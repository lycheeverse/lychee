//! Facility for cached asynchronous operations, with operations keyed by a
//! key type and ensuring mutual exclusion of operations with the same key.

use std::borrow::Borrow;
use std::hash::Hash;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use tokio::sync::watch;

/// Cache for asynchronous operations. Each operation is associated with a key,
/// and operations are cached, deduplicated and mutually exclusive with other
/// operations on the same key, including in-progress operations.
pub struct Cache<K, V> {
    /// Internal map of keys to set-once values.
    data: DashMap<K, watch::Receiver<Option<Arc<V>>>>,
    /// Number of cache hits (including hits to in-progress values).
    pub num_hits: AtomicUsize,
    /// Number of cache misses.
    pub num_misses: AtomicUsize,
}

/// A value returned on cache hits. [`CacheGetter::get`] returns a future which
/// resolves when the cache value has been stored by the corresponding [`CacheSetter`].
#[derive(Debug, Clone)]
pub struct CacheGetter<T>(watch::Receiver<Option<Arc<T>>>);

impl<T> CacheGetter<T> {
    /// Returns a future which resolves when the cache value is computed
    /// (by another task). If the value has already been computed and stored,
    /// the future will be ready immediately.
    ///
    /// # Errors
    /// Resolves to an error if the corresponding [`CacheSetter`] has been
    /// dropped without setting a value.
    pub async fn get(mut self) -> Result<Arc<T>, watch::error::RecvError> {
        let received = self.0.wait_for(Option::is_some).await?;
        let arc = received.as_ref().expect("impossible due to is_some check");
        Ok(arc.clone())
    }
}

/// A value returned on cache misses. The owner of this struct should compute
/// the value, then call [`CacheSetter::set`] to write the value into the cache.
///
/// If this struct is dropped before being written to (including due to panic),
/// the value will remain empty and associated [`CacheGetter`]s will *never resolve*.
/// This can be avoided by calling [`CacheSetter::with_fallback`] which will
/// specify a fallback closure in case it is prematurely dropped.
#[derive(Debug)]
pub struct CacheSetter<T>(watch::Sender<Option<Arc<T>>>);

impl<T> CacheSetter<T> {
    /// Constructs a new [`CacheSetter`] writing into the given [`watch::Sender`].
    #[must_use]
    pub(crate) const fn new(sender: watch::Sender<Option<Arc<T>>>) -> Self {
        Self(sender)
    }

    /// Writes the given value into the cache, consuming this [`CacheSetter`] and
    /// returning a [`CacheGetter`] referencing the stored value.
    pub fn set(self, value: T) -> CacheGetter<T> {
        self.0.send_replace(Some(Arc::new(value)));
        CacheGetter(self.0.subscribe())
    }

    /// Returns a new detached [`CacheSetter`]. That is, a setter which is
    /// not backed by any value within the cache. This can be useful to let
    /// uncacheable entities use the same cache-handling logic.
    pub fn new_detached() -> Self {
        Self(watch::channel(None).0)
    }
}

impl<K, V> Cache<K, V>
where
    K: Hash + Eq,
{
    /// Constructs a new empty [`Cache`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: DashMap::new(),
            num_hits: 0.into(),
            num_misses: 0.into(),
        }
    }

    pub(self) fn insert(&self, key: K, value: V) {
        let (_, recv) = watch::channel(Some(Arc::new(value)));
        self.data.insert(key, recv);
    }

    /// Locks the cache entry with the given key, adding it to the cache if
    /// it does not already exist. This function returns values which can be
    /// used to write into or read from the cache.
    ///
    /// If this is the first task to lock this entry, [`Ok`] of [`CacheSetter`]
    /// is returned so the call can compute and store the value. If the value is
    /// already cached or another task is currently computing the value, [`Err`]
    /// of [`CacheGetter`] is returned which can be used to wait and retrieve the value
    /// from the cache.
    ///
    /// The given key will only be cloned if the cache does not currently have
    /// an entry for this key.
    ///
    /// # Errors
    /// An [`Err`] means the cache key is already completed or in-progress, as
    /// described above.
    pub fn lock_entry<T>(&self, key: &T) -> Result<CacheSetter<V>, CacheGetter<V>>
    where
        T: ToOwned<Owned = K> + Eq + Hash + ?Sized,
        K: Borrow<T>,
    {
        if let Some(entry) = self.data.get(key.borrow()) {
            return Err(CacheGetter(entry.clone()));
        }

        match self.data.entry(key.to_owned()) {
            Entry::Vacant(vacant) => {
                self.num_misses.fetch_add(1, Ordering::Relaxed);
                let (send, recv) = watch::channel(None);
                vacant.insert(recv);
                Ok(CacheSetter::new(send))
            }
            Entry::Occupied(occupied) => {
                self.num_hits.fetch_add(1, Ordering::Relaxed);
                Err(CacheGetter(occupied.get().clone()))
            }
        }
    }
}

impl<K, V> Default for Cache<K, V>
where
    K: Hash + Eq,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> Clone for Cache<K, V>
where
    K: Hash + Eq + Clone,
{
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            num_hits: self.num_hits.load(Ordering::Relaxed).into(),
            num_misses: self.num_misses.load(Ordering::Relaxed).into(),
        }
    }
}

impl<K, V> FromIterator<(K, V)> for Cache<K, V>
where
    K: Hash + Eq,
{
    fn from_iter<It: IntoIterator<Item = (K, V)>>(iter: It) -> Self {
        let cache = Self::new();
        for (k, v) in iter {
            cache.insert(k, v);
        }
        cache
    }
}

impl<K, V> std::fmt::Debug for Cache<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cache").finish_non_exhaustive()
    }
}
