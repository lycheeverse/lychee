use std::fmt::{self, Display};

use super::StatsFormatter;
use anyhow::Result;
use http::StatusCode;
use lychee_lib::{ResponseBody, Status};
use std::fmt::Write;
use tabled::{object::Segment, Alignment, Modify, Table, Tabled};

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
            status: "\u{1f50d} Total",
            count: stats.total,
        },
        StatsTableEntry {
            status: "\u{2705} Successful",
            count: stats.successful,
        },
        StatsTableEntry {
            status: "\u{23f3} Timeouts",
            count: stats.timeouts,
        },
        StatsTableEntry {
            status: "\u{1f500} Redirected",
            count: stats.redirects,
        },
        StatsTableEntry {
            status: "\u{1f47b} Excluded",
            count: stats.excludes,
        },
        StatsTableEntry {
            status: "\u{2753} Unknown",
            count: stats.unknown,
        },
        StatsTableEntry {
            status: "\u{1f6ab} Errors",
            count: stats.errors,
        },
    ];
    let style = tabled::Style::markdown();

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
        "* [{}] [{}]({})",
        response.status.code(),
        response.uri,
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

        writeln!(f, "## Summary")?;
        writeln!(f)?;
        writeln!(f, "{}", stats_table(&self.0))?;

        if !&stats.fail_map.is_empty() {
            writeln!(f)?;
            writeln!(f, "## Errors per input\n")?;
            for (source, responses) in &stats.fail_map {
                // Using leading newlines over trailing ones (e.g. `writeln!`)
                // lets us avoid extra newlines without any additional logic.
                writeln!(f, "### Errors in {}\n", source)?;
                for response in responses {
                    writeln!(
                        f,
                        "{}",
                        markdown_response(response).map_err(|_e| fmt::Error)?
                    )?;
                }
                writeln!(f)?;
            }
        }

        Ok(())
    }
}

pub(crate) struct Markdown;

impl Markdown {
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl StatsFormatter for Markdown {
    fn format_stats(&self, stats: ResponseStats) -> Result<Option<String>> {
        let markdown = MarkdownResponseStats(stats);
        Ok(Some(markdown.to_string()))
    }
}

#[cfg(test)]
mod tests {

    use http::StatusCode;
    use lychee_lib::{CacheStatus, InputSource, Response, ResponseBody, Status, Uri};

    use super::*;

    #[test]
    fn test_markdown_response_ok() {
        let response = ResponseBody {
            uri: Uri::try_from("http://example.com").unwrap(),
            status: Status::Ok(StatusCode::OK),
        };
        let markdown = markdown_response(&response).unwrap();
        assert_eq!(
            markdown,
            "* [200] [http://example.com/](http://example.com/)"
        );
    }

    #[test]
    fn test_markdown_response_cached_ok() {
        let response = ResponseBody {
            uri: Uri::try_from("http://example.com").unwrap(),
            status: Status::Cached(CacheStatus::Ok(200)),
        };
        let markdown = markdown_response(&response).unwrap();
        assert_eq!(
            markdown,
            "* [200] [http://example.com/](http://example.com/) | Cached: OK (cached)"
        );
    }

    #[test]
    fn test_markdown_response_cached_err() {
        let response = ResponseBody {
            uri: Uri::try_from("http://example.com").unwrap(),
            status: Status::Cached(CacheStatus::Error(Some(400))),
        };
        let markdown = markdown_response(&response).unwrap();
        assert_eq!(
            markdown,
            "* [400] [http://example.com/](http://example.com/) | Cached: Error (cached)"
        );
    }

    #[test]
    fn test_render_stats() {
        let stats = ResponseStats::default();
        let table = stats_table(&stats);
        let expected = r#"| Status        | Count |
|---------------|-------|
| 🔍 Total      | 0     |
| ✅ Successful | 0     |
| ⏳ Timeouts   | 0     |
| 🔀 Redirected | 0     |
| 👻 Excluded   | 0     |
| ❓ Unknown    | 0     |
| 🚫 Errors     | 0     |"#;
        assert_eq!(table, expected.to_string());
    }

    #[test]
    fn test_render_summary() {
        let mut stats = ResponseStats::default();
        let response = Response(
            InputSource::Stdin,
            ResponseBody {
                uri: Uri::try_from("http://127.0.0.1").unwrap(),
                status: Status::Cached(CacheStatus::Error(Some(404))),
            },
        );
        stats.add(response);
        let summary = MarkdownResponseStats(stats);
        let expected = r#"## Summary

| Status        | Count |
|---------------|-------|
| 🔍 Total      | 1     |
| ✅ Successful | 0     |
| ⏳ Timeouts   | 0     |
| 🔀 Redirected | 0     |
| 👻 Excluded   | 0     |
| ❓ Unknown    | 0     |
| 🚫 Errors     | 1     |

## Errors per input

### Errors in stdin

* [404] [http://127.0.0.1/](http://127.0.0.1/) | Cached: Error (cached)

"#;
        assert_eq!(summary.to_string(), expected.to_string());
    }
}
