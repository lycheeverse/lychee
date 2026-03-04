use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use serde::{Deserialize, Serialize};
use tokio::sync::{SetOnce, SetOnceError};

use crate::time::{self, Timestamp, timestamp};
use lychee_lib::{CacheStatus, Status, StatusCodeSelector, Uri};

/// Describes a response status that can be serialized to disk
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CacheValue {
    pub(crate) status: CacheStatus,
    pub(crate) timestamp: Timestamp,
}

impl From<&Status> for CacheValue {
    fn from(s: &Status) -> Self {
        let timestamp = time::timestamp();
        CacheValue {
            status: s.into(),
            timestamp,
        }
    }
}

struct MappedValue<T, F> {
    val: T,
    f: F,
}

impl<T> MappedValue<T, fn(T) -> T> {
    fn new(val: T) -> Self {
        MappedValue { val, f: |x| x }
    }
}

fn compose<T, U: 'static, V>(f: impl Fn(&T) -> &U, g: impl Fn(&U) -> &V) -> impl Fn(&T) -> &V {
    move |x| g(f(x))
}

impl<T, U: 'static, F: Fn(&T) -> &U> MappedValue<T, F>
{
    fn map_ref<V>(self, g: impl Fn(&U) -> &V) -> MappedValue<T, impl Fn(&T) -> &V> {
        MappedValue {
            val: self.val,
            f: compose(self.f, g)
        }
    }
}

impl<T, U, F: Fn(T) -> U> MappedValue<T, F>
{
    fn map_owned<V>(self, g: impl Fn(U) -> V) -> MappedValue<T, impl Fn(T) -> V> {
        MappedValue {
            val: self.val,
            f: move |x| g((self.f)(x))
        }
    }
}

impl<T, F, U> Deref for MappedValue<T, F>
where
    F: Fn(&T) -> &U,
{
    type Target = U;
    fn deref(&self) -> &Self::Target {
        (self.f)(&self.val)
    }
}

impl<T, U, F: Fn(T) -> U> Into<U> for MappedValue<T, F>
{
    fn into(self) -> U {
        (self.f)(self.val)
    }
}

fn f() {
    MappedValue::new(vec![1]).map_owned(|x| x);
}

/// The cache stores previous response codes for faster checking.
///
/// At the moment it is backed by `DashMap`, but this is an implementation
/// detail which should not be relied upon. The cache values stored within
/// the map are wrapped in [`SetOnce`] to represent a request that might be
/// in-flight and not yet finished.
#[derive(Default, Debug)]
pub(crate) struct Cache(pub(crate) DashMap<Uri, Arc<SetOnce<CacheValue>>>);

impl Cache {
    pub(crate) fn lock_entry<Fut>(
        &self,
        uri: Uri,
        f: impl FnOnce(&Uri) -> Fut,
    ) -> impl Fn() -> Future<Output = CacheValue>
    where
        Fut: Future<Output = CacheValue>,
    {
        let arc = match self.0.entry(uri.clone()) {
            Entry::Vacant(vac) => {
                let arc = Arc::new(SetOnce::new());
                Ok(vac.insert(arc).value().clone())
            }
            Entry::Occupied(occ) => Err(occ.get().clone()),
        };

        match arc {
            Ok(arc) => {
                let mapped = MappedValue::new(arc).map_owned(move |arc: Arc<SetOnce<CacheValue>>| {
                    let arc = arc.clone();
                    let uri = uri.clone();
                    async move {
                        let arc = arc.clone();
                        let _ = arc.set(f(&uri).await);
                        arc.wait().await
                    }
                });

            }
            Err(arc) =>
            panic!(""),
        }
    }

    /// Returns whether the given [`Uri`] should bypass the cache entirely.
    pub(crate) fn is_bypassed_from_cache(uri: &Uri) -> bool {
        uri.is_file()
    }

    /// Returns `true` if the cache value should be omitted when writing the
    /// cache to disk.
    ///
    /// The response should be ignored if:
    /// - The status is excluded.
    /// - The status is unsupported.
    /// - The status is unknown.
    /// - The status code is excluded from the cache.
    fn is_omitted_from_disk_cache(cache_value: &CacheValue) -> bool {
        match cache_value.status {
            CacheStatus::Ok(_) | CacheStatus::Error(_) => true,
            CacheStatus::Excluded | CacheStatus::Unsupported => false,
        }
    }

    /// Store the cache under the given path. Update access timestamps
    pub(crate) fn store(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut wtr = csv::WriterBuilder::new()
            .has_headers(false)
            .from_path(path)?;
        for entry in &self.0 {
            if let Some(v) = entry.value().get()
                && !Self::is_omitted_from_disk_cache(v)
            {
                wtr.serialize((entry.key(), v))?;
            }
        }
        Ok(())
    }

    /// Load cache from path. Discard entries older than `max_age_secs`
    pub(crate) fn load<T: AsRef<Path>>(
        path: T,
        max_age_secs: u64,
        excluder: &StatusCodeSelector,
    ) -> Result<Cache> {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_path(path)?;

        let map = DashMap::new();
        let current_ts = timestamp();
        for result in rdr.deserialize() {
            let (uri, value): (Uri, CacheValue) = result?;
            // Discard entries older than `max_age_secs`.
            // This allows gradually updating the cache over multiple runs.
            if current_ts - value.timestamp >= max_age_secs {
                continue;
            }

            // Discard entries for status codes which have been excluded.
            // Without this check, an entry might be cached, then its status code is configured as
            // excluded, and in subsequent runs the cached value is still reused.
            if value.status.is_excluded(excluder) {
                continue;
            }

            map.insert(uri, value);
        }
        Ok(map.into())
    }
}

impl<It> From<It> for Cache
where
    It: IntoIterator<Item = (Uri, CacheValue)>,
{
    fn from(it: It) -> Self {
        let map = DashMap::new();
        for (k, v) in it {
            map.insert(k, Arc::new(v.into()));
        }
        Self(map)
    }
}

#[cfg(test)]
mod tests {
    use dashmap::DashMap;
    use http::StatusCode;
    use lychee_lib::{CacheStatus, StatusCodeSelector, StatusRange, Uri};

    use crate::{
        cache::{Cache, CacheValue, StoreExt},
        time::timestamp,
    };

    #[test]
    fn test_excluded_status_not_reused_from_cache() {
        let uri: Uri = "https://example.com".try_into().unwrap();

        let cache: Cache = Default::default();
        cache.insert(
            uri.clone(),
            CacheValue {
                status: CacheStatus::Ok(StatusCode::TOO_MANY_REQUESTS),
                timestamp: timestamp(),
            },
        );

        let tmp = tempfile::NamedTempFile::new().unwrap();
        cache.store(tmp.path()).unwrap();

        let mut excluder = StatusCodeSelector::empty();
        excluder.add_range(StatusRange::new(400, 500).unwrap());

        let cache = Cache::load(tmp.path(), u64::MAX, &excluder).unwrap();
        assert!(cache.0.get(&uri).is_none());
    }
}
