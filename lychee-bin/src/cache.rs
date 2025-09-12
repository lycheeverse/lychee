use crate::time::{self, Timestamp, timestamp};
use anyhow::Result;
use dashmap::DashMap;
use lychee_lib::{CacheStatus, Status, StatusCodeExcluder, Uri};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Describes a response status that can be serialized to disk
#[derive(Serialize, Deserialize)]
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
/// At the moment it is backed by `DashMap`, but this is an
/// implementation detail, which should not be relied upon.
pub(crate) type Cache = DashMap<Uri, CacheValue>;

pub(crate) trait StoreExt {
    /// Store the cache under the given path. Update access timestamps
    fn store<T: AsRef<Path>>(&self, path: T) -> Result<()>;

    /// Load cache from path. Discard entries older than `max_age_secs`
    fn load<T: AsRef<Path>>(
        path: T,
        max_age_secs: u64,
        excluder: &StatusCodeExcluder,
    ) -> Result<Cache>;
}

impl StoreExt for Cache {
    fn store<T: AsRef<Path>>(&self, path: T) -> Result<()> {
        let mut wtr = csv::WriterBuilder::new()
            .has_headers(false)
            .from_path(path)?;
        for result in self {
            wtr.serialize((result.key(), result.value()))?;
        }
        Ok(())
    }

    fn load<T: AsRef<Path>>(
        path: T,
        max_age_secs: u64,
        excluder: &StatusCodeExcluder,
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
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use dashmap::DashMap;
    use lychee_lib::{AcceptRange, CacheStatus, StatusCodeExcluder, Uri};

    use crate::{
        cache::{Cache, CacheValue, StoreExt},
        time::timestamp,
    };

    #[test]
    fn test_excluded_status_not_reused_from_cache() {
        let uri: Uri = "https://example.com".try_into().unwrap();

        let cache: Cache = DashMap::<Uri, CacheValue>::new();
        cache.insert(
            uri.clone(),
            CacheValue {
                status: CacheStatus::Ok(429),
                timestamp: timestamp(),
            },
        );

        let tmp = tempfile::NamedTempFile::new().unwrap();
        cache.store(tmp.path()).unwrap();

        let mut excluder = StatusCodeExcluder::new();
        excluder.add_range(AcceptRange::new_from(400, 500).unwrap());

        let cache = Cache::load(tmp.path(), u64::MAX, &excluder).unwrap();
        assert!(cache.get(&uri).is_none());
    }
}
