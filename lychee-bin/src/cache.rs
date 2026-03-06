use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use either::Either;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::sync::SetOnce;

use crate::time::{self, Timestamp, timestamp};
use lychee_lib::cache::{Cache, CacheFut, CacheSetter};
use lychee_lib::{CacheStatus, Status, StatusCodeSelector, Uri};

/// An *in-memory* cached value. Compared to the on-disk cache, this
/// stores a richer [`Status`] type for link checks which were performed
/// within the current execution of lychee.
#[derive(Debug)]
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

    /// Returns `true` if the given cache value should be omitted when writing the
    /// cache to disk.
    ///
    /// The cache value will be omitted if:
    /// - The status is excluded.
    /// - The status is unsupported.
    /// - The status is unknown.
    /// - The status code is excluded from the cache.
    pub(crate) fn to_disk_cache_value(
        cache_value: CacheValue,
        cache_exclude_status: &HashSet<StatusCode>,
    ) -> Option<(CacheStatus, Timestamp)> {
        let CacheValue { status, timestamp } = cache_value;

        if Option::<StatusCode>::from(status.clone())
            .is_some_and(|s| cache_exclude_status.contains(&s))
        {
            return Some((status, timestamp));
        }
        match status {
            CacheStatus::Ok(_) | CacheStatus::Error(_) => Some((status, timestamp)),
            CacheStatus::Excluded | CacheStatus::Unsupported => None,
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
        // for entry in self.0 {
        //     if !Self::is_omitted_from_disk_cache(v, cache_exclude_status) {
        //         wtr.serialize((entry.key(), v))?;
        //     }
        // }
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
