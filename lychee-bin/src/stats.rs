use std::collections::{HashMap, HashSet};

use lychee_lib::{CacheStatus, InputSource, Response, ResponseBody, Status};
use serde::Serialize;

#[derive(Default, Serialize, Debug)]
pub(crate) struct ResponseStats {
    pub(crate) detailed_stats: bool,
    pub(crate) total: usize,
    pub(crate) successful: usize,
    pub(crate) unknown: usize,
    pub(crate) unsupported: usize,
    pub(crate) timeouts: usize,
    pub(crate) redirects: usize,
    pub(crate) excludes: usize,
    pub(crate) errors: usize,
    pub(crate) cached: usize,
    pub(crate) success_map: HashMap<InputSource, HashSet<ResponseBody>>,
    pub(crate) fail_map: HashMap<InputSource, HashSet<ResponseBody>>,
    pub(crate) excluded_map: HashMap<InputSource, HashSet<ResponseBody>>,
}

impl ResponseStats {
    #[inline]
    pub(crate) fn extended() -> Self {
        Self {
            detailed_stats: true,
            ..Default::default()
        }
    }

    pub(crate) fn increment_status_counters(&mut self, status: &Status) {
        match status {
            Status::Ok(_) => self.successful += 1,
            Status::Error(_) => self.errors += 1,
            Status::UnknownStatusCode(_) => self.unknown += 1,
            Status::Timeout(_) => self.timeouts += 1,
            Status::Redirected(_) => self.redirects += 1,
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

    pub(crate) fn add(&mut self, response: Response) {
        self.total += 1;

        let Response(source, ResponseBody { ref status, .. }) = response;
        self.increment_status_counters(status);

        match status {
            _ if status.is_error() => {
                let fail = self.fail_map.entry(source).or_default();
                fail.insert(response.1);
            }
            Status::Ok(_) if self.detailed_stats => {
                let success = self.success_map.entry(source).or_default();
                success.insert(response.1);
            }
            Status::Excluded if self.detailed_stats => {
                let excluded = self.excluded_map.entry(source).or_default();
                excluded.insert(response.1);
            }
            _ => (),
        }
    }

    #[inline]
    pub(crate) const fn is_success(&self) -> bool {
        self.total == self.successful + self.excludes
    }

    #[inline]
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
        let response_body = ResponseBody { uri, status };
        Response(InputSource::Stdin, response_body)
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

        let Response(source, body) = dummy_error();
        let expected_fail_map: HashMap<InputSource, HashSet<ResponseBody>> =
            HashMap::from_iter([(source, HashSet::from_iter([body]))]);
        assert_eq!(stats.fail_map, expected_fail_map);

        assert!(stats.success_map.is_empty());
    }

    #[tokio::test]
    async fn test_detailed_stats() {
        let mut stats = ResponseStats::extended();
        assert!(stats.success_map.is_empty());
        assert!(stats.fail_map.is_empty());
        assert!(stats.excluded_map.is_empty());

        stats.add(dummy_error());
        stats.add(dummy_excluded());
        stats.add(dummy_ok());

        let mut expected_fail_map: HashMap<InputSource, HashSet<ResponseBody>> = HashMap::new();
        let Response(source, response_body) = dummy_error();
        let entry = expected_fail_map.entry(source).or_default();
        entry.insert(response_body);
        assert_eq!(stats.fail_map, expected_fail_map);

        let mut expected_success_map: HashMap<InputSource, HashSet<ResponseBody>> = HashMap::new();
        let Response(source, response_body) = dummy_ok();
        let entry = expected_success_map.entry(source).or_default();
        entry.insert(response_body);
        assert_eq!(stats.success_map, expected_success_map);

        let mut expected_excluded_map: HashMap<InputSource, HashSet<ResponseBody>> = HashMap::new();
        let Response(source, response_body) = dummy_excluded();
        let entry = expected_excluded_map.entry(source).or_default();
        entry.insert(response_body);
        assert_eq!(stats.excluded_map, expected_excluded_map);
    }
}
