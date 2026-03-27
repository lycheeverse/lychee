use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use http::StatusCode;

use crate::time::{self, Timestamp, timestamp};
use lychee_lib::cache::{Cache, CacheFut, CacheSetter};
use lychee_lib::{CacheStatus, Status, StatusCodeSelector, Uri};

/// An *in-memory* cached value. Compared to the on-disk cache, this
/// stores a richer [`Status`] type for link checks which were performed
/// within the current execution of lychee.
#[derive(Clone, Copy, Debug)]
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
#[derive(Default, Debug)]
pub(crate) struct LycheeCache(Cache<Uri, CacheValue>);

impl LycheeCache {
    pub(crate) fn lock_entry(
        &self,
        uri: &Uri,
    ) -> Result<CacheSetter<CacheValue>, CacheFut<CacheValue>> {
        if Self::is_bypassed_from_cache(uri) {
            // make a no-op setter that is not stored in the cache
            return Ok(CacheSetter::dissociated());
        }
        self.0.lock_entry(uri)
    }

    /// Returns whether the given [`Uri`] should bypass the cache entirely.
    /// It will always be re-executed by lychee, even within the same lychee
    /// run.
    pub(crate) fn is_bypassed_from_cache(uri: &Uri) -> bool {
        uri.is_file()
    }

    /// Returns `Some` if the given cache value should be kept when writing the
    /// cache to disk.
    ///
    /// The cache value will be omitted (and this function will return `None`) if:
    /// - The status is excluded.
    /// - The status is unsupported.
    /// - The status is unknown.
    /// - The status code is excluded from the cache.
    pub(crate) fn to_disk_cache_value(
        cache_value: CacheValue,
        cache_exclude_status: &HashSet<StatusCode>,
    ) -> Option<(CacheStatus, Timestamp)> {
        let CacheValue { status, timestamp } = cache_value;

        if Option::<StatusCode>::from(status).is_some_and(|s| cache_exclude_status.contains(&s)) {
            return Some((status, timestamp));
        }
        match status {
            CacheStatus::Ok(_) | CacheStatus::Error(_) => Some((status, timestamp)),
            CacheStatus::Excluded | CacheStatus::Unsupported => None,
        }
    }

    /// Store the cache under the given path. Update access timestamps
    pub(crate) fn store(
        self,
        path: impl AsRef<Path>,
        cache_exclude_status: &HashSet<StatusCode>,
    ) -> Result<()> {
        let mut wtr = csv::WriterBuilder::new()
            .has_headers(false)
            .from_path(path)?;

        for (k, v) in self.0.into_completed_entries() {
            // Do not serialize errors to disk. We always want to recheck failing links.
            if matches!(v.status, CacheStatus::Error(_)) {
                continue;
            }
            if let Some(v) = Self::to_disk_cache_value(v, cache_exclude_status) {
                wtr.serialize((k, v))?;
            }
        }

        Ok(())
    }

    /// Load cache from path. Discard entries older than `max_age_secs`
    pub(crate) fn load<T: AsRef<Path>>(
        path: T,
        max_age_secs: u64,
        excluder: &StatusCodeSelector,
    ) -> Result<LycheeCache> {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_path(path)?;

        let mut data = vec![];
        let current_ts = timestamp();
        for result in rdr.deserialize() {
            let (uri, status, timestamp): (Uri, CacheStatus, Timestamp) = result?;
            // Discard entries older than `max_age_secs`.
            // This allows gradually updating the cache over multiple runs.
            if current_ts - timestamp >= max_age_secs {
                continue;
            }

            // Discard errors. Caching errors goes against typical CI workflows.
            // If a link fails due to a network issue, a server outage, or if a previously
            // failing link has been fixed, reading an error from the cache prevents lychee
            // from realizing the link is now working.
            if matches!(value.status, CacheStatus::Error(_)) {
                continue;
            }

            // Discard entries for status codes which have been excluded.
            // Without this check, an entry might be cached, then its status code is configured as
            // excluded, and in subsequent runs the cached value is still reused.
            if status.is_excluded(excluder) {
                continue;
            }

            data.push((uri, CacheValue { status, timestamp }));
        }
        Ok(Self(data.into_iter().collect()))
    }
}

impl FromIterator<(Uri, CacheValue)> for LycheeCache {
    fn from_iter<It: IntoIterator<Item = (Uri, CacheValue)>>(iter: It) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use http::StatusCode;
    use std::collections::HashSet;

    use crate::LycheeCache;
    use crate::{cache::CacheValue, time::timestamp};
    use lychee_lib::{CacheStatus, StatusCodeSelector, StatusRange, Uri};

    #[test]
    fn test_excluded_status_not_reused_from_cache() {
        let uri: Uri = "https://example.com".try_into().unwrap();

        let cache: LycheeCache = vec![(
            uri.clone(),
            CacheValue {
                status: CacheStatus::Ok(StatusCode::TOO_MANY_REQUESTS),
                timestamp: timestamp(),
            },
        )]
        .into_iter()
        .collect();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        cache.store(tmp.path(), &HashSet::default()).unwrap();

        let mut excluder = StatusCodeSelector::empty();
        excluder.add_range(StatusRange::new(400, 500).unwrap());

        let cache = LycheeCache::load(tmp.path(), u64::MAX, &excluder).unwrap();
        assert!(cache.lock_entry(&uri).is_ok());
    }

    #[test]
    fn test_errors_not_stored_in_cache() {
        let uri: Uri = "https://example.com/error".try_into().unwrap();

        let cache = Cache::new();
        cache.insert(
            uri.clone(),
            CacheValue {
                status: CacheStatus::Error(Some(StatusCode::INTERNAL_SERVER_ERROR)),
                timestamp: timestamp(),
            },
        );
        let uri_none: Uri = "https://example.com/none".try_into().unwrap();
        cache.insert(
            uri_none.clone(),
            CacheValue {
                status: CacheStatus::Error(None),
                timestamp: timestamp(),
            },
        );

        let tmp = tempfile::NamedTempFile::new().unwrap();
        cache.store(tmp.path()).unwrap();

        let excluder = StatusCodeSelector::empty();
        let loaded_cache = Cache::load(tmp.path(), u64::MAX, &excluder).unwrap();
        assert!(loaded_cache.get(&uri).is_none());
        assert!(loaded_cache.get(&uri_none).is_none());
    }
}
