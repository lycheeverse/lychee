use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Display},
};

use super::StatsFormatter;
use anyhow::Result;
use http::StatusCode;
use lychee_lib::{InputSource, ResponseBody, Status};
use std::fmt::Write;
use tabled::{
    Table, Tabled,
    settings::{Alignment, Modify, Style, object::Segment},
};

use crate::stats::ResponseStats;

#[derive(Tabled)]
struct StatsTableEntry {
    #[tabled(rename = "Status")]
    status: &'static str,
    #[tabled(rename = "Count")]
    count: usize,
}

fn stats_table(stats: &ResponseStats) -> String {
    let stats = vec![
        StatsTableEntry {
            status: "🔍 Total",
            count: stats.total,
        },
        StatsTableEntry {
            status: "✅ Successful",
            count: stats.successful,
        },
        StatsTableEntry {
            status: "⏳ Timeouts",
            count: stats.timeouts,
        },
        StatsTableEntry {
            status: "🔀 Redirected",
            count: stats.redirects,
        },
        StatsTableEntry {
            status: "👻 Excluded",
            count: stats.excludes,
        },
        StatsTableEntry {
            status: "❓ Unknown",
            count: stats.unknown,
        },
        StatsTableEntry {
            status: "🚫 Errors",
            count: stats.errors,
        },
        StatsTableEntry {
            status: "⛔ Unsupported",
            count: stats.unsupported,
        },
    ];
    let style = Style::markdown();

    Table::new(stats)
        .with(Modify::new(Segment::all()).with(Alignment::left()))
        .with(style)
        .to_string()
}

/// Helper function to format single response body as markdown
///
/// Optional details get added if available.
fn markdown_response(response: &ResponseBody) -> Result<String> {
    let mut formatted = format!(
        "* [{}] <{}>",
        response.status.code_as_string(),
        response.uri,
    );

    if let Status::Ok(StatusCode::OK) = response.status {
        // Don't print anything else if the status code is 200.
        // The output gets too verbose then.
        return Ok(formatted);
    }

    // Add a separator between the URI and the additional details below.
    // Note: To make the links clickable in some terminals,
    // we add a space before the separator.
    write!(formatted, " | {}", response.status)?;

    if let Some(details) = response.status.details() {
        write!(formatted, ": {details}")?;
    }
    Ok(formatted)
}

struct MarkdownResponseStats(ResponseStats);

impl Display for MarkdownResponseStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stats = &self.0;

        writeln!(f, "# Summary")?;
        writeln!(f)?;
        writeln!(f, "{}", stats_table(&self.0))?;

        write_stats_per_input(f, "Errors", &stats.error_map, |response| {
            markdown_response(response).map_err(|_| fmt::Error)
        })?;

        write_stats_per_input(f, "Redirects", &stats.redirect_map, |response| {
            markdown_response(response).map_err(|_| fmt::Error)
        })?;

        write_stats_per_input(f, "Suggestions", &stats.suggestion_map, |suggestion| {
            Ok(format!(
                "* {} --> {}",
                suggestion.original, suggestion.suggestion
            ))
        })?;

        Ok(())
    }
}

fn write_stats_per_input<T, F>(
    f: &mut fmt::Formatter<'_>,
    name: &'static str,
    map: &HashMap<InputSource, HashSet<T>>,
    write_stat: F,
) -> fmt::Result
where
    T: Display,
    F: Fn(&T) -> Result<String, std::fmt::Error>,
{
    if !&map.is_empty() {
        writeln!(f, "\n## {name} per input")?;
        for (source, responses) in super::sort_stat_map(map) {
            writeln!(f, "\n### {name} in {source}\n")?;
            for response in responses {
                writeln!(f, "{}", write_stat(response)?)?;
            }
        }
    }

    Ok(())
}

pub(crate) struct Markdown;

impl Markdown {
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl StatsFormatter for Markdown {
    fn format(&self, stats: ResponseStats) -> Result<Option<String>> {
        let markdown = MarkdownResponseStats(stats);
        Ok(Some(markdown.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use http::StatusCode;
    use lychee_lib::{CacheStatus, InputSource, Redirects, Response, ResponseBody, Status, Uri};
    use reqwest::Url;

    use crate::formatters::suggestion::Suggestion;

    use super::*;

    #[test]
    fn test_markdown_response_ok() {
        let response = ResponseBody {
            uri: Uri::try_from("http://example.com").unwrap(),
            status: Status::Ok(StatusCode::OK),
        };
        let markdown = markdown_response(&response).unwrap();
        assert_eq!(markdown, "* [200] <http://example.com/>");
    }

    #[test]
    fn test_markdown_response_cached_ok() {
        let response = ResponseBody {
            uri: Uri::try_from("http://example.com").unwrap(),
            status: Status::Cached(CacheStatus::Ok(200)),
        };
        let markdown = markdown_response(&response).unwrap();
        assert_eq!(markdown, "* [200] <http://example.com/> | OK (cached)");
    }

    #[test]
    fn test_markdown_response_cached_err() {
        let response = ResponseBody {
            uri: Uri::try_from("http://example.com").unwrap(),
            status: Status::Cached(CacheStatus::Error(Some(400))),
        };
        let markdown = markdown_response(&response).unwrap();
        assert_eq!(markdown, "* [400] <http://example.com/> | Error (cached)");
    }

    #[test]
    fn test_render_stats() {
        let stats = ResponseStats::default();
        let table = stats_table(&stats);
        let expected = "| Status         | Count |
|----------------|-------|
| 🔍 Total       | 0     |
| ✅ Successful  | 0     |
| ⏳ Timeouts    | 0     |
| 🔀 Redirected  | 0     |
| 👻 Excluded    | 0     |
| ❓ Unknown     | 0     |
| 🚫 Errors      | 0     |
| ⛔ Unsupported | 0     |";
        assert_eq!(table, expected.to_string());
    }

    #[test]
    fn test_render_summary() {
        let mut stats = ResponseStats::default();

        // Add cached error
        stats.add(Response::new(
            Uri::try_from("http://127.0.0.1").unwrap(),
            Status::Cached(CacheStatus::Error(Some(404))),
            InputSource::Stdin,
        ));

        // Add suggestion
        stats
            .suggestion_map
            .entry((InputSource::Stdin).clone())
            .or_default()
            .insert(Suggestion {
                suggestion: Url::parse("https://example.com/suggestion").unwrap(),
                original: Url::parse("https://example.com/original").unwrap(),
            });

        // Add redirect
        stats.add(Response::new(
            Uri::try_from("http://redirected.dev").unwrap(),
            Status::Redirected(
                StatusCode::OK,
                Redirects::from(vec![
                    Url::parse("https://1.dev").unwrap(),
                    Url::parse("https://2.dev").unwrap(),
                    Url::parse("http://redirected.dev").unwrap(),
                ]),
            ),
            InputSource::Stdin,
        ));

        let summary = MarkdownResponseStats(stats);
        let expected = "# Summary

| Status         | Count |
|----------------|-------|
| 🔍 Total       | 2     |
| ✅ Successful  | 0     |
| ⏳ Timeouts    | 0     |
| 🔀 Redirected  | 1     |
| 👻 Excluded    | 0     |
| ❓ Unknown     | 0     |
| 🚫 Errors      | 1     |
| ⛔ Unsupported | 0     |

## Errors per input

### Errors in stdin

* [404] <http://127.0.0.1/> | Error (cached)

## Redirects per input

### Redirects in stdin

* [200] <http://redirected.dev/> | Redirect: Followed 2 redirects resolving to the final status of: OK. Redirects: https://1.dev/ --> https://2.dev/ --> http://redirected.dev/

## Suggestions per input

### Suggestions in stdin

* https://example.com/original --> https://example.com/suggestion
";
        assert_eq!(summary.to_string(), expected.to_string());
    }
}
