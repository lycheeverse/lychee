mod compact;
mod detailed;
mod json;
mod junit;
mod markdown;
mod raw;
mod response;

pub(crate) use compact::Compact;

pub(crate) use detailed::Detailed;
pub(crate) use json::Json;
pub(crate) use junit::Junit;
pub(crate) use markdown::Markdown;
pub(crate) use raw::Raw;
pub(crate) use response::ResponseStats;
use serde::Serialize;

use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    fs,
    io::{Write, stdout},
};

use crate::{formatters::get_stats_formatter, options::Config};
use anyhow::{Context, Result};
use lychee_lib::{InputSource, ratelimit::HostStatsMap};

#[derive(Default, Serialize)]
pub(crate) struct OutputStats {
    #[serde(flatten)]
    pub(crate) response_stats: ResponseStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) host_stats: Option<HostStatsMap>,
}

pub(crate) trait StatsFormatter {
    /// Format the stats of all responses and write them to stdout
    fn format(&self, stats: OutputStats) -> Result<String>;
}

/// If configured to do so, output response statistics to stdout or the specified output file.
pub(crate) fn output_statistics(stats: OutputStats, config: &Config) -> Result<()> {
    let formatter = get_stats_formatter(&config.format(), &config.mode());
    let formatted_stats = formatter.format(stats)?;

    if let Some(output) = &config.output {
        fs::write(output, formatted_stats).context("Cannot write status output to file")?;
    } else {
        // we assume that the formatted stats don't have a final newline
        writeln!(stdout(), "{formatted_stats}")?;
    }
    Ok(())
}

/// Convert a `ResponseStats` `HashMap` to a sorted Vec of key-value pairs
/// The returned keys and values are both sorted in natural, case-insensitive order
fn sort_stat_map<T>(stat_map: &HashMap<InputSource, HashSet<T>>) -> Vec<(&InputSource, Vec<&T>)>
where
    T: Display,
{
    sort_stat_maps(&vec![&stat_map])
}

/// Merge `ResponseStats` `HashMap` to a sorted Vec of key-value pairs
/// The returned keys and values are both sorted in natural, case-insensitive order
fn sort_stat_maps<'a, T>(
    stat_maps: &Vec<&'a HashMap<InputSource, HashSet<T>>>,
) -> Vec<(&'a InputSource, Vec<&'a T>)>
where
    T: Display,
{
    // First we collect all unique sources across all maps, then we sort them in natural order.
    let all_sources_hs: HashSet<_> = stat_maps.iter().flat_map(|m| m.keys()).collect();
    let mut all_sources: Vec<_> = all_sources_hs.into_iter().collect();

    all_sources.sort_by_key(|item| item.to_string().to_lowercase());

    let entries: Vec<_> = all_sources
        .into_iter()
        .map(|source| {
            let mut responses: Vec<&T> = stat_maps
                .iter()
                .filter_map(|m| m.get(source))
                .flat_map(|set| set.iter())
                .collect();

            responses.sort_by_key(|item| item.to_string().to_lowercase());

            (source, responses)
        })
        .collect();

    entries
}

#[cfg(test)]
fn get_dummy_stats() -> OutputStats {
    use std::{num::NonZeroUsize, time::Duration};

    use http::StatusCode;
    use lychee_lib::{RawUriSpan, Redirect, Redirects, ResponseBody, Status, ratelimit::HostStats};
    use url::Url;

    use crate::formatters::suggestion::Suggestion;

    const SPAN: Option<RawUriSpan> = Some(RawUriSpan {
        column: Some(NonZeroUsize::MIN),
        line: NonZeroUsize::MIN,
    });
    const DURATION: Option<Duration> = Some(Duration::from_secs(1));

    let source = InputSource::RemoteUrl(Box::new(Url::parse("https://example.com").unwrap()));
    let error_map = HashMap::from([(
        source.clone(),
        HashSet::from([ResponseBody {
            uri: "https://github.com/mre/idiomatic-rust-doesnt-exist-man"
                .try_into()
                .unwrap(),
            status: Status::Ok(StatusCode::NOT_FOUND),
            span: SPAN,
            duration: DURATION,
        }]),
    )]);

    let timeout_map = HashMap::from([(
        source.clone(),
        HashSet::from([ResponseBody {
            uri: "https://httpbin.org/delay/2".try_into().unwrap(),
            status: Status::Timeout(None),
            span: SPAN,
            duration: DURATION,
        }]),
    )]);

    let suggestion_map = HashMap::from([(
        source.clone(),
        HashSet::from([Suggestion {
            original: "https://original.dev".try_into().unwrap(),
            suggestion: "https://suggestion.dev".try_into().unwrap(),
        }]),
    )]);

    let mut redirects = Redirects::new("https://1.dev".try_into().unwrap());
    redirects.push(Redirect {
        url: "https://2.dev".try_into().unwrap(),
        code: StatusCode::PERMANENT_REDIRECT,
    });
    redirects.push(Redirect {
        url: "http://redirected.dev".try_into().unwrap(),
        code: StatusCode::PERMANENT_REDIRECT,
    });

    let redirect_map = HashMap::from([(
        source.clone(),
        HashSet::from([ResponseBody {
            uri: "https://redirected.dev".try_into().unwrap(),
            status: Status::Redirected(StatusCode::OK, redirects),
            span: SPAN,
            duration: DURATION,
        }]),
    )]);

    let response_stats = ResponseStats {
        total: 2,
        successful: 0,
        errors: 1,
        unknown: 0,
        excludes: 0,
        timeouts: 1,
        duration: Duration::ZERO,
        unsupported: 0,
        redirects: 1,
        cached: 0,
        suggestion_map,
        redirect_map,
        success_map: HashMap::default(),
        error_map,
        excluded_map: HashMap::default(),
        timeout_map,
        detailed_stats: true,
    };

    let host_stats = Some(HostStatsMap::from(HashMap::from([(
        String::from("example.com"),
        HostStats {
            total_requests: 5,
            successful_requests: 3,
            rate_limited: 1,
            server_errors: 1,
            cache_hits: 1,
            cache_misses: 4,
            ..Default::default()
        },
    )])));

    OutputStats {
        response_stats,
        host_stats,
    }
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

        Response::new(uri, Status::Error(ErrorKind::EmptyUrl), source, None, None)
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
