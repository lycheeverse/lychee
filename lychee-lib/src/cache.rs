//! Facility for cached asynchronous operations, with operations keyed by a
//! key type and ensuring mutual exclusion of operations with the same key.

use std::hash::Hash;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use tokio::sync::SetOnce;

/// Cache for asynchronous operations, with each operation associated with a key.
/// Operations are cached, deduplicated and mutually exclusive with other
/// operations on the same key, including in-progress operations.
pub struct Cache<K, V> {
    /// Internal map of keys to set-once values.
    data: DashMap<K, Arc<SetOnce<V>>>,
    /// Number of cache hits (including hits to in-progress values).
    pub num_hits: AtomicUsize,
    /// Number of cache misses.
    pub num_misses: AtomicUsize,
}

/// A future returned on cache hits. [`CacheFut::wait`] returns a future which
/// resolves when the cache value has been computed by another task.
#[derive(Debug)]
pub struct CacheFut<V>(Arc<SetOnce<V>>);

impl<T> CacheFut<T> {
    /// Returns a future which resolves when the cache value is computed
    /// (by another task). If the value has already been computed and stored,
    /// the future will be ready immediately.
    pub fn wait(&self) -> impl Future<Output = &T> {
        async { self.0.wait().await }
    }
}

/// A value returned on cache misses. The owner of this struct should compute
/// the value, then call [`CacheSetter::set`] to write the value into the cache.
///
/// If this struct is dropped before being written to (including due to panic),
/// the value will remain empty and associated [`CacheFut`]s will *never resolve*.
/// This can be avoided by calling [`CacheSetter::with_fallback`] which will
/// specify a fallback closure in case it is prematurely dropped.
#[derive(Debug)]
pub struct CacheSetter<T, Fn: FnOnce() -> T = fn() -> T>(Arc<SetOnce<T>>, Option<Fn>);

impl<T, Fn: FnOnce() -> T> CacheSetter<T, Fn> {
    /// Constructs a new [`CacheSetter`] writing into the given [`SetOnce`].
    ///
    /// By default, no fallback is configured.
    pub fn new(arc: Arc<SetOnce<T>>) -> Self {
        Self(arc, None)
    }

    /// Returns a new [`CacheSetter`] with the configured fallback closure and
    /// writing into the same [`SetOnce`].
    pub fn with_fallback<F: FnOnce() -> T>(mut self, default: F) -> CacheSetter<T, F> {
        let arc = std::mem::take(&mut self.0);
        self.1 = None;
        CacheSetter(arc, Some(default))
    }

    /// Writes the given value into the cache, consuming this [`CacheSetter`].
    pub fn set(self, value: T) {
        let _ = self.0.set(value);
    }

    /// Returns a new dissociated [`CacheSetter`]. That is, a setter which is
    /// not backed by any value within the cache. This can be useful to let
    /// uncacheable entities use the same cache-handling logic.
    pub fn dissociated() -> Self {
        Self(Default::default(), None)
    }
}

/// Drop implementation that calls the stored [`CacheSetter::with_fallback`]
/// closure, if it is configured and no value has been manually stored.
impl<T, Fn: FnOnce() -> T> Drop for CacheSetter<T, Fn> {
    fn drop(&mut self) {
        if let Some(f) = self.1.take()
            && !self.0.initialized()
        {
            let _ = self.0.set(f());
        }
    }
}

impl<K, V> Cache<K, V>
where
    K: Hash + Eq,
{
    /// Constructs a new empty [`Cache`].
    pub fn new() -> Self {
        Self {
            data: DashMap::new(),
            num_hits: 0.into(),
            num_misses: 0.into(),
        }
    }

    /// Locks the cache entry with the given key, returning [`Ok`] if this
    /// is the first task to lock this entry (and so, the value should be computed),
    /// or [`Err`] if the value is already cached or another task is currently
    /// computing the value.
    ///
    /// The given key will only be cloned if the cache does not currently have
    /// an entry for this key.
    pub fn lock_entry(&self, key: &K) -> Result<CacheSetter<V>, CacheFut<V>>
    where
        K: Clone,
    {
        if let Some(entry) = self.data.get(key) {
            return Err(CacheFut(entry.value().clone()));
        }

        match self.data.entry(key.clone()) {
            Entry::Vacant(vac) => {
                self.num_misses.fetch_add(1, Ordering::Relaxed);
                let arc = vac.insert(Default::default()).value().clone();
                Ok(CacheSetter::new(arc))
            }
            Entry::Occupied(occ) => {
                self.num_hits.fetch_add(1, Ordering::Relaxed);
                Err(CacheFut(occ.get().clone()))
            }
        }
    }

    /// Consumes the cache and returns an iterator over the completed key
    /// and value pairs.
    pub fn into_completed_entries(self) -> impl Iterator<Item = (K, V)> {
        self.data.into_iter().filter_map(|(k, v)| {
            let cell = Arc::into_inner(v).expect("unresolved CacheFut or CacheSetter values exist");
            match cell.into_inner() {
                Some(x) => Some((k, x)),
                None => None,
            }
        })
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

impl<K, V> FromIterator<(K, V)> for Cache<K, V>
where
    K: Hash + Eq,
{
    fn from_iter<It: IntoIterator<Item = (K, V)>>(iter: It) -> Self {
        let cache = Self::new();
        for (k, v) in iter {
            cache.data.insert(k, Arc::new(v.into()));
        }
        cache
    }
}

impl<K, V> std::fmt::Debug for Cache<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cache").finish_non_exhaustive()
    }
}
