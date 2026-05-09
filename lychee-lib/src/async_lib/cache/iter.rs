//! Types which implement the iterators for [`super::Cache`].

use std::iter::FilterMap;
use std::sync::Arc;
use std::{fmt::Debug, hash::Hash, ops::Deref};

use dashmap::DashMap;
use dashmap::mapref::multiple::RefMulti;

use super::CacheGetter;

/// A borrowed reference to a key within the [`super::Cache`].
pub struct KeyRef<'a, K, V>(RefMulti<'a, K, CacheGetter<V>>);

impl<K: Hash + Eq, V> Deref for KeyRef<'_, K, V> {
    type Target = K;
    fn deref(&self) -> &K {
        self.0.key()
    }
}

impl<K: Debug + Hash + Eq, V> Debug for KeyRef<'_, K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("KeyRef").field(&**self).finish()
    }
}

/// An iterator yielding owned key-value pairs for each completed and non-shared
/// value within the cache. This is returned by [`super::Cache::into_iter`].
#[allow(clippy::type_complexity)]
#[must_use]
pub struct OwningIter<K, V>(
    FilterMap<
        dashmap::iter::OwningIter<K, CacheGetter<V>>,
        fn((K, CacheGetter<V>)) -> Option<(K, V)>,
    >,
);

impl<K: Hash + Eq, V> OwningIter<K, V> {
    pub(super) fn new(map: DashMap<K, CacheGetter<V>>) -> Self {
        let make_owned_pair = |(k, getter): (K, CacheGetter<V>)| {
            let arc = getter.into_inner()?;
            let v = Arc::into_inner(arc)?;
            Some((k, v))
        };

        Self(map.into_iter().filter_map(make_owned_pair))
    }
}

impl<K: Hash + Eq, V> Iterator for OwningIter<K, V> {
    type Item = (K, V);
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<K, V> Debug for OwningIter<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("OwningIter").finish_non_exhaustive()
    }
}

/// An iterator yielding borrowed key-value pairs for each completed value within
/// the cache. This is returned by [`super::Cache::iter`].
///
/// This acquires a read lock of the cache while iterating.
#[allow(clippy::type_complexity)]
#[must_use]
pub struct Iter<'a, K, V>(
    FilterMap<
        dashmap::iter::Iter<'a, K, CacheGetter<V>>,
        fn(RefMulti<'a, K, CacheGetter<V>>) -> Option<(KeyRef<'a, K, V>, Arc<V>)>,
    >,
);

impl<'a, K: Hash + Eq, V> Iter<'a, K, V> {
    pub(super) fn new(map: &'a DashMap<K, CacheGetter<V>>) -> Self {
        let make_borrowed_pair = |mapref: RefMulti<'a, K, CacheGetter<V>>| {
            let arc = mapref.value().get()?;
            Some((KeyRef(mapref), arc))
        };

        Self(map.iter().filter_map(make_borrowed_pair))
    }
}

impl<'a, K: Hash + Eq, V> Iterator for Iter<'a, K, V> {
    type Item = (KeyRef<'a, K, V>, Arc<V>);
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<K, V> Debug for Iter<'_, K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Iter").finish_non_exhaustive()
    }
}
