use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::sync::SetOnce;

use crate::time::{self, Timestamp, timestamp};
use lychee_lib::{CacheStatus, Status, StatusCodeSelector, Uri};

/// Describes a response status that can be serialized to disk
#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
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

/// The cache stores previous response codes for faster checking.
///
/// At the moment it is backed by `DashMap`, but this is an implementation
/// detail which should not be relied upon. The cache values stored within
/// the map are wrapped in [`SetOnce`] to represent a request that might be
/// in-flight and not yet finished.
#[derive(Default, Debug)]
pub(crate) struct Cache(pub(crate) DashMap<Uri, Arc<SetOnce<CacheValue>>>);

pub(crate) struct CacheFut(Arc<SetOnce<CacheValue>>);

impl CacheFut {
    pub(crate) fn wait(&self) -> impl Future<Output = &CacheValue> {
        async { self.0.wait().await }
    }
}

pub(crate) struct CacheSetter<T, Fn: FnOnce() -> T = fn() -> T>(Arc<SetOnce<T>>, Option<Fn>);

impl<T, Fn: FnOnce() -> T> CacheSetter<T, Fn> {
    fn empty_arc() -> Arc<SetOnce<T>> {
        Default::default()
    }

    pub(crate) fn dissociated() -> Self {
        Self(Self::empty_arc(), None)
    }

    pub(crate) fn new(arc: Arc<SetOnce<T>>) -> Self {
        Self(arc, None)
    }

    pub(crate) fn with_fallback<F: FnOnce() -> T>(mut self, default: F) -> CacheSetter<T, F> {
        let arc = std::mem::take(&mut self.0);
        self.1 = None;
        CacheSetter(arc, Some(default))
    }

    pub(crate) fn set(self, value: T) {
        let _ = self.0.set(value);
    }
}

impl<T, Fn: FnOnce() -> T> Drop for CacheSetter<T, Fn> {
    fn drop(&mut self) {
        if let Some(f) = self.1.take()
            && !self.0.initialized()
        {
            let _ = self.0.set(f());
        }
    }
}

impl Cache {
    fn make_setter(arc: Arc<SetOnce<CacheValue>>) -> impl FnOnce(CacheValue) -> () {
        move |x| {
            arc.set(x)
                .expect("cache already set?? this should not happen because of mutual exclusion")
        }
    }

    pub(crate) fn lock_entry(&self, uri: Uri) -> Result<CacheSetter<CacheValue>, CacheFut> {
        if Self::is_bypassed_from_cache(&uri) {
            // make a no-op setter that is not stored in the cache
            return Ok(CacheSetter::dissociated());
        }
        match self.0.entry(uri) {
            Entry::Vacant(vac) => {
                let arc = vac.insert(Default::default()).value().clone();
                Ok(CacheSetter::new(arc))
            }
            Entry::Occupied(occ) => Err(CacheFut(occ.get().clone())),
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
    pub(crate) fn is_omitted_from_disk_cache(cache_value: &CacheValue) -> bool {
        match cache_value.status {
            CacheStatus::Ok(_) | CacheStatus::Error(_) => false,
            CacheStatus::Excluded | CacheStatus::Unsupported => true,
        }
    }

    /// Store the cache under the given path. Update access timestamps
    pub(crate) fn store(
        &self,
        path: impl AsRef<Path>,
        cache_exclude_status: &HashSet<StatusCode>,
    ) -> Result<()> {
        let mut wtr = csv::WriterBuilder::new()
            .has_headers(false)
            .from_path(path)?;
        for entry in &self.0 {
            if let Some(v) = entry.value().get()
                && !Self::is_omitted_from_disk_cache(v)
            {
                if Option::<StatusCode>::from(v.status)
                    .is_none_or(|s| !cache_exclude_status.contains(&s))
                {
                    wtr.serialize((entry.key(), v))?;
                }
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
        Ok(map.into_iter().collect())
    }
}

impl FromIterator<(Uri, CacheValue)> for Cache {
    fn from_iter<It: IntoIterator<Item = (Uri, CacheValue)>>(iter: It) -> Self {
        let map = DashMap::new();
        for (k, v) in iter {
            map.insert(k, Arc::new(v.into()));
        }
        Self(map)
    }
}

#[cfg(test)]
mod tests {
    use http::StatusCode;
    use lychee_lib::{CacheStatus, StatusCodeSelector, StatusRange, Uri};

    use crate::{
        cache::{Cache, CacheValue},
        time::timestamp,
    };

    #[test]
    fn test_excluded_status_not_reused_from_cache() {
        let uri: Uri = "https://example.com".try_into().unwrap();

        let cache: Cache = vec![(
            uri.clone(),
            CacheValue {
                status: CacheStatus::Ok(StatusCode::TOO_MANY_REQUESTS),
                timestamp: timestamp(),
            },
        )]
        .into_iter()
        .collect();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        cache.store(tmp.path(), &Default::default()).unwrap();

        let mut excluder = StatusCodeSelector::empty();
        excluder.add_range(StatusRange::new(400, 500).unwrap());

        let cache = Cache::load(tmp.path(), u64::MAX, &excluder).unwrap();
        assert!(cache.0.get(&uri).is_none());
    }
}
