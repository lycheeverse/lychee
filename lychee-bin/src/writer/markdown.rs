use std::fmt::{self, Display};

use super::StatsWriter;
use anyhow::Result;
use tabled::{style::Line, Alignment, Full, Modify, Table, Tabled};

use crate::stats::ResponseStats;

#[derive(Tabled)]
struct StatsTableEntry {
    #[header("Status")]
    status: &'static str,
    #[header("Count")]
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
    let style = tabled::Style::github_markdown().header(Some(Line::bordered('-', '|', '|', '|')));

    Table::new(stats)
        .with(Modify::new(Full).with(Alignment::left()))
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
            for (input, responses) in &stats.fail_map {
                // Using leading newlines over trailing ones (e.g. `writeln!`)
                // lets us avoid extra newlines without any additional logic.
                writeln!(f, "### Errors in {}", input)?;
                for response in responses {
                    writeln!(
                        f,
                        "* [{}]({}): {}",
                        response.uri, response.uri, response.status
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
        Markdown {}
    }
}

impl StatsWriter for Markdown {
    fn write(&self, stats: ResponseStats) -> Result<String> {
        let markdown = MarkdownResponseStats(stats);
        Ok(markdown.to_string())
    }
}
