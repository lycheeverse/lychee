//! Facility for cached asynchronous operations, with operations keyed by a
//! key type and ensuring mutual exclusion of operations with the same key.

use core::fmt::Debug;
use std::borrow::Borrow;
use std::hash::Hash;
use std::iter::FilterMap;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use dashmap::mapref::multiple::RefMulti;
use tokio::sync::watch;

pub mod entry;

use entry::{CacheGetter, CacheSetter};

/// Cache for asynchronous operations. Each operation is associated with a key,
/// and operations are cached, deduplicated and mutually exclusive with other
/// operations on the same key, including in-progress operations.
pub struct Cache<K, V> {
    /// Internal map of keys to the getter for that key.
    data: DashMap<K, CacheGetter<V>>,
    /// Number of cache hits (including hits to in-progress values). This
    /// corresponds to the number of [`CacheGetter`]s returned by the cache.
    pub num_hits: AtomicUsize,
    /// Number of cache misses. This corresponds to the number of
    /// [`CacheSetter`]s returned by the cache.
    pub num_misses: AtomicUsize,
}

impl<K: Hash + Eq, V> Cache<K, V> {
    /// Constructs a new empty [`Cache`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: DashMap::new(),
            num_hits: 0.into(),
            num_misses: 0.into(),
        }
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
    /// This acquires a read lock which is upgraded to a write lock if an entry
    /// needs to be inserted.
    ///
    /// # Errors
    /// An [`Err`] means the cache key is already completed or in-progress, as
    /// described above.
    pub fn lock_entry<Q>(&self, key: &Q) -> Result<CacheSetter<V>, CacheGetter<V>>
    where
        Q: ToOwned<Owned = K> + Eq + Hash + ?Sized,
        K: Borrow<Q>,
    {
        if let Some(getter) = self.get_entry(key) {
            return Err(getter);
        }

        match self.data.entry(key.to_owned()) {
            Entry::Vacant(vacant) => {
                self.num_misses.fetch_add(1, Ordering::Relaxed);
                let (send, recv) = watch::channel(None);
                vacant.insert(CacheGetter::new(recv));
                Ok(CacheSetter::new(send))
            }
            Entry::Occupied(occupied) => {
                self.num_hits.fetch_add(1, Ordering::Relaxed);
                Err(occupied.get().clone())
            }
        }
    }

    /// Gets the cache entry with the given key, if it is completed
    /// or in-progress. Returns [`None`] if the key does not exist.
    ///
    /// This acquires a read lock of the cache.
    pub fn get_entry<Q>(&self, key: &Q) -> Option<CacheGetter<V>>
    where
        Q: Hash + Eq + ?Sized,
        K: Borrow<Q>,
    {
        let getter = self.data.get(key.borrow())?;
        self.num_hits.fetch_add(1, Ordering::Relaxed);
        Some(getter.clone())
    }

    /// Returns an iterator yielding borrowed key-value pairs for each
    /// completed entry within the cache. See also [`Cache::into_iter`].
    pub fn iter(&self) -> Iter<'_, K, V> {
        self.into_iter()
    }

    /// Inserts the given key-value pair, overwriting any existing values.
    /// Does not modify any counters in the cache.
    fn insert(&self, key: K, value: V) {
        let (_, recv) = watch::channel(Some(Arc::new(value)));
        self.data.insert(key, CacheGetter::new(recv));
    }
}

impl<K: Hash + Eq, V> Default for Cache<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Clone + Hash + Eq, V> Clone for Cache<K, V> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            num_hits: self.num_hits.load(Ordering::Relaxed).into(),
            num_misses: self.num_misses.load(Ordering::Relaxed).into(),
        }
    }
}

impl<K, V> Debug for Cache<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Destructure will detect changes to the fields in future.
        let Cache {
            data: _,
            num_hits,
            num_misses,
        } = self;

        f.debug_struct("Cache")
            .field("num_hits", num_hits)
            .field("num_misses", num_misses)
            .finish_non_exhaustive()
    }
}

impl<K: Hash + Eq, V> Extend<(K, V)> for Cache<K, V> {
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        iter.into_iter().for_each(|(k, v)| self.insert(k, v));
    }
}

impl<K: Hash + Eq, V> FromIterator<(K, V)> for Cache<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let cache = Self::new();
        iter.into_iter().for_each(|(k, v)| cache.insert(k, v));
        cache
    }
}

/// Converts the [`Cache`] into an iterator yielding owned key-value pairs
/// for each completed and non-shared value within the cache.
impl<K: Hash + Eq, V> IntoIterator for Cache<K, V> {
    type Item = (K, V);
    type IntoIter = IntoIter<K, V>;
    fn into_iter(self) -> Self::IntoIter {
        let make_owned_pair = |(k, getter): (K, CacheGetter<V>)| -> Option<Self::Item> {
            let arc = getter.into_inner()?;
            let v = Arc::into_inner(arc)?;
            Some((k, v))
        };

        self.data.into_iter().filter_map(make_owned_pair)
    }
}

/// Returns an iterator yielding borrowed key-value pairs for each
/// completed value within the cache.
///
/// This acquires a read lock of the cache while iterating.
impl<'a, K: Hash + Eq, V> IntoIterator for &'a Cache<K, V> {
    type Item = (KeyRef<'a, K, CacheGetter<V>>, Arc<V>);
    type IntoIter = Iter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        let make_borrowed_pair = |mapref: RefMulti<'a, K, CacheGetter<V>>| -> Option<Self::Item> {
            let arc = mapref.value().get()?;
            Some((KeyRef(mapref), arc))
        };

        self.data.iter().filter_map(make_borrowed_pair)
    }
}

/// Iterator returned by [`Cache::into_iter`].
pub type IntoIter<K, V> = FilterMap<
    dashmap::iter::OwningIter<K, CacheGetter<V>>,
    fn((K, CacheGetter<V>)) -> Option<(K, V)>,
>;

/// Iterator returned by [`Cache::iter`].
pub type Iter<'a, K, V> = FilterMap<
    dashmap::iter::Iter<'a, K, CacheGetter<V>>,
    fn(RefMulti<'a, K, CacheGetter<V>>) -> Option<(KeyRef<'a, K, CacheGetter<V>>, Arc<V>)>,
>;

/// Helper type that wraps a dashmap [`RefMulti`] and changes its
/// [`Deref`] implementation to return the key rather than the value.
pub struct KeyRef<'a, K: Hash + Eq, V>(RefMulti<'a, K, V>);

impl<K: Hash + Eq, V> Deref for KeyRef<'_, K, V> {
    type Target = K;

    fn deref(&self) -> &Self::Target {
        self.0.key()
    }
}

impl<K: Debug + Hash + Eq, V> Debug for KeyRef<'_, K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("KeyRef").field(&**self).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::Cache;

    #[test]
    fn test_cache_usize_key() {
        let cache = Cache::<usize, usize>::new();
        cache.lock_entry(&0).unwrap().set(0);
    }

    #[test]
    fn test_cache_key_borrow() {
        let cache = Cache::<String, usize>::new();
        let str_ref = "str ref";
        let string = "string".to_string();

        cache.lock_entry(str_ref).unwrap().set(0);
        cache.lock_entry(&string).unwrap().set(1);

        assert_eq!(cache.into_iter().count(), 2);
    }
}
