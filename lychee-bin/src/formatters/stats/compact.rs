use anyhow::Result;
use console::Style;
use std::{
    fmt::{self, Display},
    sync::LazyLock,
    time::Duration,
};

use crate::formatters::{
    color::{BOLD_GREEN, BOLD_PINK, BOLD_YELLOW, DIM, NORMAL, color},
    get_response_formatter,
    host_stats::CompactHostStats,
    stats::{OutputStats, ResponseStats},
};
use crate::options;

use super::StatsFormatter;

struct CompactResponseStats {
    stats: ResponseStats,
    mode: options::OutputMode,
}

impl Display for CompactResponseStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stats = &self.stats;

        if !stats.error_map.is_empty() {
            let input = if stats.error_map.len() == 1 {
                "input"
            } else {
                "inputs"
            };

            color!(
                f,
                BOLD_PINK,
                "Issues found in {} {input}. Find details below.\n\n",
                stats.error_map.len()
            )?;
        }

        let response_formatter = get_response_formatter(&self.mode);

        for (source, responses) in super::sort_stat_map(&stats.error_map) {
            color!(f, BOLD_YELLOW, "[{}]:\n", source)?;
            for response in responses {
                writeln!(f, "{}", response_formatter.format_response(response))?;
            }

            if let Some(suggestions) = stats.suggestion_map.get(source) {
                // Sort suggestions
                let mut sorted_suggestions: Vec<_> = suggestions.iter().collect();
                sorted_suggestions.sort_by(|a, b| {
                    let (a, b) = (a.to_string().to_lowercase(), b.to_string().to_lowercase());
                    numeric_sort::cmp(&a, &b)
                });

                writeln!(f, "\nℹ Suggestions")?;
                for suggestion in sorted_suggestions {
                    writeln!(f, "{suggestion}")?;
                }
            }

            writeln!(f)?;
        }

        color!(f, NORMAL, "🔍 {} Total", stats.total)?;

        // show duration (in a human readable format), e.g. 2m 30s
        let duration = Duration::from_secs(stats.duration_secs);
        color!(f, DIM, " (in {})", humantime::format_duration(duration))?;

        color!(f, BOLD_GREEN, " ✅ {} OK", stats.successful)?;

        let total_errors = stats.errors;

        let err_str = if total_errors == 1 { "Error" } else { "Errors" };
        color!(f, BOLD_PINK, " 🚫 {} {}", total_errors, err_str)?;

        write_if_any(stats.unknown, "❓", "Unknown", &BOLD_PINK, f)?;
        write_if_any(stats.excludes, "👻", "Excluded", &BOLD_YELLOW, f)?;
        write_if_any(stats.timeouts, "⏳", "Timeouts", &BOLD_YELLOW, f)?;
        write_if_any(stats.unsupported, "⛔", "Unsupported", &BOLD_YELLOW, f)?;
        write_if_any(stats.redirects, "🔀", "Redirects", &BOLD_YELLOW, f)?;

        Ok(())
    }
}

fn write_if_any(
    value: usize,
    symbol: &str,
    text: &str,
    style: &LazyLock<Style>,
    f: &mut fmt::Formatter<'_>,
) -> Result<(), fmt::Error> {
    if value > 0 {
        color!(f, style, " {} {} {}", symbol, value, text)?;
    }
    Ok(())
}

pub(crate) struct Compact {
    mode: options::OutputMode,
}

impl Compact {
    pub(crate) const fn new(mode: options::OutputMode) -> Self {
        Self { mode }
    }
}

impl StatsFormatter for Compact {
    fn format(&self, stats: OutputStats) -> Result<String> {
        let response_stats = CompactResponseStats {
            stats: stats.response_stats,
            mode: self.mode.clone(),
        };
        let host_stats = CompactHostStats {
            host_stats: stats.host_stats,
        };

        Ok(format!("{response_stats}\n{host_stats}"))
    }
}

#[cfg(test)]
mod tests {
    use crate::formatters::stats::{StatsFormatter, get_dummy_stats};
    use crate::options::OutputMode;
    use regex::Regex;

    use super::*;

    #[test]
    fn test_formatter() {
        let formatter = Compact::new(OutputMode::Plain);
        let result = formatter.format(get_dummy_stats()).unwrap();

        // Remove color codes for better readability of the expected result
        let without_color_codes = Regex::new(r"\u{1b}\[[0-9;]*m")
            .unwrap()
            .replace_all(&result, "")
            .to_string();

        assert_eq!(
            without_color_codes,
            "Issues found in 1 input. Find details below.

[https://example.com/]:
[404] https://github.com/mre/idiomatic-rust-doesnt-exist-man | 404 Not Found: Not Found

ℹ Suggestions
https://original.dev/ --> https://suggestion.dev/

🔍 2 Total (in 0s) ✅ 0 OK 🚫 1 Error 🔀 1 Redirects

📊 Per-host Statistics
────────────────────────────────────────────────────────────
example.com   │      5 reqs │   60.0% success │      N/A median │   20.0% cached
"
        );
    }
}
