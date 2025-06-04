mod compact;
mod detailed;
mod json;
mod markdown;
mod raw;

pub(crate) use compact::Compact;
pub(crate) use detailed::Detailed;
pub(crate) use json::Json;
pub(crate) use markdown::Markdown;
pub(crate) use raw::Raw;

use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
};

use crate::stats::ResponseStats;
use anyhow::Result;
use lychee_lib::InputSource;

pub(crate) trait StatsFormatter {
    /// Format the stats of all responses and write them to stdout
    fn format(&self, stats: ResponseStats) -> Result<Option<String>>;
}

/// Convert a `ResponseStats` `HashMap` to a sorted Vec of key-value pairs
/// The returned keys and values are both sorted in natural, case-insensitive order
fn sort_stat_map<T>(stat_map: &HashMap<InputSource, HashSet<T>>) -> Vec<(&InputSource, Vec<&T>)>
where
    T: Display,
{
    let mut entries: Vec<_> = stat_map
        .iter()
        .map(|(source, responses)| {
            let mut sorted_responses: Vec<&T> = responses.iter().collect();
            sorted_responses.sort_by(|a, b| {
                let (a, b) = (a.to_string().to_lowercase(), b.to_string().to_lowercase());
                human_sort::compare(&a, &b)
            });

            (source, sorted_responses)
        })
        .collect();

    entries.sort_by(|(a, _), (b, _)| {
        let (a, b) = (a.to_string().to_lowercase(), b.to_string().to_lowercase());
        human_sort::compare(&a, &b)
    });

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    use lychee_lib::{ErrorKind, Response, Status, Uri};
    use url::Url;

    fn make_test_url(url: &str) -> Url {
        Url::parse(url).expect("Expected valid Website URI")
    }

    fn make_test_response(url_str: &str, source: InputSource) -> Response {
        let uri = Uri::from(make_test_url(url_str));

        Response::new(uri, Status::Error(ErrorKind::InvalidUrlHost), source)
    }

    #[test]
    fn test_sorted_stat_map() {
        let mut test_stats = ResponseStats::default();

        // Sorted list of test sources
        let test_sources = vec![
            InputSource::RemoteUrl(Box::new(make_test_url("https://example.com/404"))),
            InputSource::RemoteUrl(Box::new(make_test_url("https://example.com/home"))),
            InputSource::RemoteUrl(Box::new(make_test_url("https://example.com/page/1"))),
            InputSource::RemoteUrl(Box::new(make_test_url("https://example.com/page/10"))),
        ];

        // Sorted list of test responses
        let test_response_urls = vec![
            "https://example.com/",
            "https://github.com/",
            "https://itch.io/",
            "https://youtube.com/",
        ];

        // Add responses to stats
        // Responses are added to a HashMap, so the order is not preserved
        for source in &test_sources {
            for response in &test_response_urls {
                test_stats.add(make_test_response(response, source.clone()));
            }
        }

        // Sort error map and extract the sources
        let sorted_errors = sort_stat_map(&test_stats.error_map);
        let sorted_sources: Vec<InputSource> = sorted_errors
            .iter()
            .map(|(source, _)| (*source).clone())
            .collect();

        // Check that the input sources are sorted
        assert_eq!(test_sources, sorted_sources);

        // Check that the responses are sorted
        for (_, response_bodies) in sorted_errors {
            let response_urls: Vec<&str> = response_bodies
                .into_iter()
                .map(|response| response.uri.as_str())
                .collect();

            assert_eq!(test_response_urls, response_urls);
        }
    }
}
