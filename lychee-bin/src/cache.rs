use crate::time::{self, Timestamp, timestamp};
use anyhow::Result;
use dashmap::{DashMap, mapref::one::Ref};
use http::StatusCode;
use lychee_lib::{CacheStatus, Client, Request, Response, Status, StatusCodeSelector, Uri};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path};

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
#[derive(Default)]
pub(crate) struct Cache(DashMap<Uri, CacheValue>);

impl Cache {
    pub(crate) fn new() -> Self {
        Self(DashMap::new())
    }

    fn insert(&self, key: Uri, value: CacheValue) -> Option<CacheValue> {
        self.0.insert(key, value)
    }

    fn get(&self, key: &Uri) -> Option<Ref<'_, Uri, CacheValue>> {
        self.0.get(key)
    }

    #[cfg(test)]
    pub(crate) fn contains_key(&self, key: &Uri) -> bool {
        self.0.contains_key(key)
    }

    /// Store the cache under the given path. Update access timestamps
    pub(crate) fn store(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut wtr = csv::WriterBuilder::new()
            .has_headers(false)
            .from_path(path)?;

        for result in &self.0 {
            // Do not serialize errors to disk. We always want to recheck failing links.
            if matches!(result.value().status, CacheStatus::Error(_)) {
                continue;
            }
            wtr.serialize((result.key(), result.value()))?;
        }

        Ok(())
    }

    /// Load cache from path. Discard entries older than `max_age_secs`
    pub(crate) fn load(
        path: impl AsRef<Path>,
        max_age_secs: u64,
        excluder: &StatusCodeSelector,
    ) -> Result<Cache> {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_path(path)?;

        let map = Cache::new();
        let current_ts = timestamp();

        for result in rdr.deserialize() {
            let (uri, value): (Uri, CacheValue) = result?;
            // Discard entries older than `max_age_secs`.
            // This allows gradually updating the cache over multiple runs.
            if current_ts - value.timestamp >= max_age_secs {
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
            if value.status.is_excluded(excluder) {
                continue;
            }

            map.insert(uri, value);
        }

        Ok(map)
    }

    pub(crate) async fn handle<F: Future<Output = Response>>(
        &self,
        client: &Client,
        cache_exclude_status: HashSet<StatusCode>,
        accept: &HashSet<StatusCode>,
        request: Request,
        check: impl Fn(Request) -> F,
    ) -> lychee_lib::Result<Response> {
        // The cache key should be the actual URL which gets requested, i.e. after remaps.
        // If using the original URL then the cache is at risk of being incorrect if remaps
        // change between runs. The cached status code comes from the actual URL anyway.
        let cache_key = {
            let mut uri = request.uri.clone();
            client.remap(&mut uri).is_ok().then_some(uri)
        };

        if let Some(cache_key) = &cache_key
            && let Some(r) = self.get(cache_key)
        {
            return Ok(cache_hit(client, accept, request, cache_key, r.value()));
        }

        let response = check(request).await;
        let status = response.status();

        if let Some(cache_key) = cache_key
            && !should_ignore(&cache_key, status, &cache_exclude_status)
        {
            self.insert(cache_key, status.into());
        }

        Ok(response)
    }
}

fn cache_hit(
    client: &Client,
    accept: &HashSet<StatusCode>,
    request: Request,
    cache_key: &Uri,
    value: &CacheValue,
) -> Response {
    let status = if client.is_excluded(cache_key) {
        Status::Excluded
    } else {
        // Can't impl `Status::from(v.value().status)` here because the
        // `accepted` status codes might have changed from the previous run
        // and they may have an impact on the interpretation of the status
        // code.
        client.host_pool().record_persistent_cache_hit(cache_key);
        Status::from_cache_status(value.status, accept)
    };

    Response::new(
        cache_key.clone(),
        status,
        request.source.into(),
        request.span,
        None,
    )
}

/// Returns `true` if the resulting [`Status`] associated to the [`Uri`]
/// should not be cached.
fn should_ignore(uri: &Uri, status: &Status, cache_exclude_status: &HashSet<StatusCode>) -> bool {
    // - Never cache filesystem access as it is fast already so caching has no benefit.
    // - Skip caching unsupported URLs as they might be supported in a future run.
    // - Skip caching excluded links; they might not be excluded in the next run.
    // - Skip caching links for which the status code has been explicitly excluded from the cache.

    let status_code_excluded = status
        .code()
        .is_some_and(|code| cache_exclude_status.contains(&code));

    uri.is_file()
        || status.is_excluded()
        || status.is_unsupported()
        || status.is_unknown()
        || status_code_excluded
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use http::StatusCode;
    use lychee_lib::{CacheStatus, ErrorKind, Status, StatusCodeSelector, StatusRange, Uri};

    use crate::{
        cache::{Cache, CacheValue, should_ignore},
        time::timestamp,
    };

    #[test]
    fn test_excluded_status_not_reused_from_cache() {
        let uri: Uri = "https://example.com".try_into().unwrap();

        let cache = Cache::new();
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
        assert!(cache.get(&uri).is_none());
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

    #[test]
    fn test_cache_by_default() {
        assert!(!should_ignore(
            &Uri::try_from("https://[::1]").unwrap(),
            &Status::Ok(StatusCode::OK),
            &HashSet::default()
        ));
    }

    #[test]
    // Cache is ignored for file URLs
    fn test_cache_ignore_file_urls() {
        assert!(should_ignore(
            &Uri::try_from("file:///home").unwrap(),
            &Status::Ok(StatusCode::OK),
            &HashSet::default()
        ));
    }

    #[test]
    // Cache is ignored for unsupported status
    fn test_cache_ignore_unsupported_status() {
        assert!(should_ignore(
            &Uri::try_from("https://[::1]").unwrap(),
            &Status::Unsupported(ErrorKind::EmptyUrl),
            &HashSet::default()
        ));
    }

    #[test]
    // Cache is ignored for unknown status
    fn test_cache_ignore_unknown_status() {
        assert!(should_ignore(
            &Uri::try_from("https://[::1]").unwrap(),
            &Status::UnknownStatusCode(StatusCode::IM_A_TEAPOT),
            &HashSet::default()
        ));
    }

    #[test]
    fn test_cache_ignore_excluded_status() {
        // Cache is ignored for excluded status codes
        let exclude = HashSet::from([StatusCode::OK]);

        assert!(should_ignore(
            &Uri::try_from("https://[::1]").unwrap(),
            &Status::Ok(StatusCode::OK),
            &exclude
        ));
    }
}
