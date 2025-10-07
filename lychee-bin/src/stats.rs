// Disable lint, clippy thinks that InputSource has inner mutability, but this seems like a false positive
#![allow(clippy::mutable_key_type)]

use std::collections::{HashMap, HashSet};

use lychee_lib::{CacheStatus, InputSource, Response, ResponseBody, Status};
use serde::Serialize;

use crate::formatters::suggestion::Suggestion;

/// Response statistics
///
/// This struct contains various counters for the responses received during a
/// run. It also contains maps to store the responses for each status (success,
/// error, excluded, etc.) and the sources of the responses.
///
/// The `detailed_stats` field is used to enable or disable the storage of the
/// responses in the maps for successful and excluded responses. If it's set to
/// `false`, the maps will be empty and only the counters will be updated.
#[derive(Default, Serialize, Debug)]
pub(crate) struct ResponseStats {
    /// Total number of responses
    pub(crate) total: usize,
    /// Number of successful responses
    pub(crate) successful: usize,
    /// Number of responses with an unknown status code
    pub(crate) unknown: usize,
    /// Number of responses, which lychee does not support right now
    pub(crate) unsupported: usize,
    /// Number of timeouts
    pub(crate) timeouts: usize,
    /// Redirects encountered while checking links
    pub(crate) redirects: usize,
    /// Number of links excluded from the run (e.g. due to the `--exclude` flag)
    pub(crate) excludes: usize,
    /// Number of responses with an error status
    pub(crate) errors: usize,
    /// Number of responses that were cached from a previous run
    pub(crate) cached: usize,
    /// Store successful responses (if `detailed_stats` is enabled)
    pub(crate) success_map: HashMap<InputSource, HashSet<ResponseBody>>,
    /// Store failed responses (if `detailed_stats` is enabled)
    pub(crate) error_map: HashMap<InputSource, HashSet<ResponseBody>>,
    /// Replacement suggestions for failed responses (if `--suggest` is enabled)
    pub(crate) suggestion_map: HashMap<InputSource, HashSet<Suggestion>>,
    /// Store redirected responses (if `detailed_stats` is enabled)
    pub(crate) redirect_map: HashMap<InputSource, HashSet<ResponseBody>>,
    /// Store excluded responses (if `detailed_stats` is enabled)
    pub(crate) excluded_map: HashMap<InputSource, HashSet<ResponseBody>>,
    /// Used to store the duration of the run in seconds.
    pub(crate) duration_secs: u64,
    /// Also track successful and excluded responses
    pub(crate) detailed_stats: bool,
}

impl ResponseStats {
    #[inline]
    /// Create a new `ResponseStats` instance with extended statistics counters
    /// enabled
    pub(crate) fn extended() -> Self {
        Self {
            detailed_stats: true,
            ..Default::default()
        }
    }

    /// Increment the counters for the given status
    ///
    /// This function is used to update the counters (success, error, etc.)
    /// based on the given response status.
    pub(crate) const fn increment_status_counters(&mut self, status: &Status) {
        match status {
            Status::Ok(_) => self.successful += 1,
            Status::Error(_) | Status::RequestError(_) => self.errors += 1,
            Status::UnknownStatusCode(_) => self.unknown += 1,
            Status::Timeout(_) => self.timeouts += 1,
            Status::Redirected(_, _) => self.redirects += 1,
            Status::Excluded => self.excludes += 1,
            Status::Unsupported(_) => self.unsupported += 1,
            Status::Cached(cache_status) => {
                self.cached += 1;
                match cache_status {
                    CacheStatus::Ok(_) => self.successful += 1,
                    CacheStatus::Error(_) => self.errors += 1,
                    CacheStatus::Excluded => self.excludes += 1,
                    CacheStatus::Unsupported => self.unsupported += 1,
                }
            }
        }
    }

    /// Add a response status to the appropriate map (success, fail, excluded)
    fn add_response_status(&mut self, response: Response) {
        let status = response.status();
        let source: InputSource = response.source().clone();
        let status_map_entry = match status {
            _ if status.is_error() => self.error_map.entry(source).or_default(),
            Status::Ok(_) if self.detailed_stats => self.success_map.entry(source).or_default(),
            Status::Excluded if self.detailed_stats => self.excluded_map.entry(source).or_default(),
            Status::Redirected(_, _) => self.redirect_map.entry(source).or_default(),
            _ => return,
        };
        status_map_entry.insert(response.1);
    }

    /// Update the stats with a new response
    pub(crate) fn add(&mut self, response: Response) {
        self.total += 1;
        self.increment_status_counters(response.status());
        self.add_response_status(response);
    }

    #[inline]
    /// Check if the entire run was successful
    pub(crate) const fn is_success(&self) -> bool {
        self.total == self.successful + self.excludes + self.unsupported + self.redirects
    }

    #[inline]
    /// Check if no responses were received
    pub(crate) const fn is_empty(&self) -> bool {
        self.total == 0
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use std::collections::{HashMap, HashSet};

    use http::StatusCode;
    use lychee_lib::{ErrorKind, InputSource, Response, ResponseBody, Status, Uri};
    use reqwest::Url;

    use super::ResponseStats;

    fn website(url: &str) -> Uri {
        Uri::from(Url::parse(url).expect("Expected valid Website URI"))
    }

    // Generate a fake response with a given status code
    // Don't use a mock server for this, as it's not necessary
    // and it's a lot faster to just generate a fake response
    fn mock_response(status: Status) -> Response {
        let uri = website("https://some-url.com/ok");
        Response::new(uri, status, InputSource::Stdin)
    }

    fn dummy_ok() -> Response {
        mock_response(Status::Ok(StatusCode::OK))
    }

    fn dummy_error() -> Response {
        mock_response(Status::Error(ErrorKind::InvalidStatusCode(1000)))
    }

    fn dummy_excluded() -> Response {
        mock_response(Status::Excluded)
    }

    #[tokio::test]
    async fn test_stats_is_empty() {
        let mut stats = ResponseStats::default();
        assert!(stats.is_empty());

        stats.add(dummy_error());

        assert!(!stats.is_empty());
    }

    #[tokio::test]
    async fn test_stats() {
        let mut stats = ResponseStats::default();
        assert!(stats.success_map.is_empty());
        assert!(stats.excluded_map.is_empty());

        stats.add(dummy_error());
        stats.add(dummy_ok());

        let response = dummy_error();
        let expected_error_map: HashMap<InputSource, HashSet<ResponseBody>> =
            HashMap::from_iter([(response.source().clone(), HashSet::from_iter([response.1]))]);
        assert_eq!(stats.error_map, expected_error_map);

        assert!(stats.success_map.is_empty());
    }

    #[tokio::test]
    async fn test_detailed_stats() {
        let mut stats = ResponseStats::extended();
        assert!(stats.success_map.is_empty());
        assert!(stats.error_map.is_empty());
        assert!(stats.excluded_map.is_empty());

        stats.add(dummy_error());
        stats.add(dummy_excluded());
        stats.add(dummy_ok());

        let mut expected_error_map: HashMap<InputSource, HashSet<ResponseBody>> = HashMap::new();
        let response = dummy_error();
        let entry = expected_error_map
            .entry(response.source().clone())
            .or_default();
        entry.insert(response.1);
        assert_eq!(stats.error_map, expected_error_map);

        let mut expected_success_map: HashMap<InputSource, HashSet<ResponseBody>> = HashMap::new();
        let response = dummy_ok();
        let entry = expected_success_map
            .entry(response.source().clone())
            .or_default();
        entry.insert(response.1);
        assert_eq!(stats.success_map, expected_success_map);

        let mut expected_excluded_map: HashMap<InputSource, HashSet<ResponseBody>> = HashMap::new();
        let response = dummy_excluded();
        let entry = expected_excluded_map
            .entry(response.source().clone())
            .or_default();
        entry.insert(response.1);
        assert_eq!(stats.excluded_map, expected_excluded_map);
    }
}
