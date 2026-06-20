//! Facility for cached asynchronous operations, with operations keyed by a
//! key type and ensuring mutual exclusion of operations with the same key.

use std::borrow::Borrow;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;

mod entry;

use dashmap::mapref::multiple::RefMulti;
pub use entry::{CacheGetter, CacheSetter};

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
                let setter = CacheSetter::new_detached();
                vacant.insert(setter.subscribe());
                Ok(setter)
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
    pub fn iter(&self) -> <&Cache<K, V> as IntoIterator>::IntoIter {
        self.into_iter()
    }

    /// Consumes the [`Cache`] and returns an iterator over (completed
    /// and non-shared) key-value pairs within the cache.
    ///
    /// Completed means entries which have been resolved (using the [`CacheSetter`]),
    /// and non-shared means there are no outstanding [`CacheGetter`] or
    /// [`Arc`] references which point to the entry's `V` result.
    pub fn into_iter_completed(self) -> Box<dyn Iterator<Item = (K, V)>>
    where
        K: 'static,
        V: 'static,
    {
        let make_owned_pair = |(k, getter): (K, CacheGetter<V>)| {
            let arc = getter.into_inner()?;
            let v = Arc::into_inner(arc)?;
            Some((k, v))
        };

        Box::new(self.data.into_iter().filter_map(make_owned_pair))
    }

    /// Inserts the given key-value pair, overwriting any existing values.
    /// Does not modify any counters in the cache.
    fn insert(&self, key: K, value: V) {
        self.data.insert(key, CacheGetter::ready(value));
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

impl<'a, K: Hash + Eq, V: 'a> IntoIterator for &'a Cache<K, V> {
    type Item = RefMulti<'a, K, CacheGetter<V>>;
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.data.iter())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

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

    #[test]
    fn test_cache_iters() {
        let cache = Cache::<&'static str, usize>::new();

        cache.lock_entry(&"completed").unwrap().set(0);
        let slow = cache.lock_entry(&"slow").unwrap();

        cache.lock_entry(&"completed but shared").unwrap().set(0);
        let _shared = cache.lock_entry(&"completed but shared").unwrap_err();

        assert_eq!(
            cache.iter().map(|x| *x.key()).collect::<BTreeSet<_>>(),
            ["completed", "completed but shared", "slow"]
                .into_iter()
                .collect(),
            "borrow iter includes even non-completed values"
        );

        slow.set(2);

        assert_eq!(
            cache.iter().map(|x| *x.key()).collect::<BTreeSet<_>>(),
            ["completed", "completed but shared", "slow"]
                .into_iter()
                .collect(),
        );

        assert_eq!(
            cache
                .into_iter_completed()
                .map(|x| x.0)
                .collect::<BTreeSet<_>>(),
            ["completed", "slow"].into_iter().collect(),
            "owning iter excludes shared values (those with outstanding getters/setters)"
        );
    }
}
