use std::fmt::{self, Display};

use super::StatsFormatter;
use anyhow::Result;
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
            count: stats.errors + stats.failures,
        },
    ];
    let style = tabled::Style::github_markdown().header_intersection('|');

    Table::new(stats)
        .with(Modify::new(Segment::all()).with(Alignment::left()))
        .with(style)
        .to_string()
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
            writeln!(f, "## Errors per input")?;
            for (source, responses) in &stats.fail_map {
                // Using leading newlines over trailing ones (e.g. `writeln!`)
                // lets us avoid extra newlines without any additional logic.
                writeln!(f, "### Errors in {}", source)?;
                for response in responses {
                    writeln!(
                        f,
                        "* [{}]({}): {} (status code: {})",
                        response.uri,
                        response.uri,
                        response.status,
                        response.status.code()
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
    use super::*;

    #[test]
    fn test_render_stats() {
        let stats = ResponseStats::new();
        let table = stats_table(&stats);
        let expected = r#"| Status        | Count |
|---------------|-------|
| 🔍 Total      | 0     |
| ✅ Successful | 0     |
| ⏳ Timeouts   | 0     |
| 🔀 Redirected | 0     |
| 👻 Excluded   | 0     |
| ❓ Unknown    | 0     |
| 🚫 Errors     | 0     |
"#;
        assert_eq!(table, expected.to_string());
    }
}
