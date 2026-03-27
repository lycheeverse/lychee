mod compact;
mod detailed;
mod json;
mod junit;
mod markdown;
mod response;

pub(crate) use compact::Compact;

pub(crate) use detailed::Detailed;
pub(crate) use json::Json;
pub(crate) use junit::Junit;
pub(crate) use markdown::Markdown;
pub(crate) use response::ResponseStats;
use serde::Serialize;

use std::{
    cmp::Eq,
    collections::{HashMap, HashSet},
    fmt::Display,
    fs,
    hash::Hash,
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

/// Convert a `ResponseStats` `HashMap` to a sorted Vec of key-value pairs.
/// The returned keys and values are both sorted in natural, case-insensitive order.
/// Additionally, the returned keys are deduplicated and their values are merged.
fn sort_stat_map<T>(stat_map: &HashMap<InputSource, HashSet<T>>) -> Vec<(&InputSource, Vec<&T>)>
where
    T: Display,
{
    sort_stats_iter(stat_map)
}

/// Sorts the given iterable of key-valuelist pairs by concatenating value lists
/// for keys which are equal. The returned keys, and the value list for each key,
/// are both sorted by case-insensitive order of their string representation.
///
/// This function takes [`IntoIterator`], so it can be called with an iterable
/// value (e.g., a [`HashMap`]) or with an iterator already constructed (e.g.,
/// from `hashmap.iter()`).
///
/// To sort multiple maps, you should chain their iterables, e.g.:
/// ```
/// let map1 = HashMap::<String, Vec<String>>::new();
/// let map2 = HashMap::<String, Vec<String>>::new();
/// let _ = sort_stats_iter(map1.iter().chain(map2.iter()));
/// ```
/// In the above example, we also see that although this function takes an
/// iterable of *borrowed* items, it can still be called with an owned iterable
/// because `.iter()` will borrow for us.
fn sort_stats_iter<'a, K, V, Entries, Vals>(it: Entries) -> Vec<(&'a K, Vec<&'a V>)>
where
    K: Display + Hash + Eq + 'a,
    V: Display + 'a,
    Entries: IntoIterator<Item = (&'a K, Vals)>,
    Vals: IntoIterator<Item = &'a V>,
{
    let mut map: HashMap<&K, Vec<&V>> = HashMap::new();
    for (k, vs) in it {
        map.entry(k).or_default().extend(vs);
    }

    let mut entries: Vec<(&K, Vec<&V>)> = map.into_iter().collect();

    // Sort first by case-insensitive string representation, then by original string representation to break ties
    entries.sort_by_cached_key(|(x, _)| (x.to_string().to_lowercase(), x.to_string()));
    for (_, vs) in &mut entries {
        vs.sort_by_cached_key(|x| (x.to_string().to_lowercase(), x.to_string()));
    }

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
