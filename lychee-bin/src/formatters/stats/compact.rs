use anyhow::Result;
use console::Style;
use once_cell::sync::Lazy;
use std::{
    fmt::{self, Display},
    time::Duration,
};

use crate::formatters::color::{color, BOLD_GREEN, BOLD_PINK, BOLD_YELLOW, DIM, NORMAL};
use crate::{formatters::get_response_formatter, options, stats::ResponseStats};

use super::StatsFormatter;

struct CompactResponseStats {
    stats: ResponseStats,
    mode: options::OutputMode,
}

impl Display for CompactResponseStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stats = &self.stats;

        if !stats.fail_map.is_empty() {
            let input = if stats.fail_map.len() == 1 {
                "input"
            } else {
                "inputs"
            };

            color!(
                f,
                BOLD_PINK,
                "Issues found in {} {input}. Find details below.\n\n",
                stats.fail_map.len()
            )?;
        }

        let response_formatter = get_response_formatter(&self.mode);

        for (source, responses) in &stats.fail_map {
            color!(f, BOLD_YELLOW, "[{}]:\n", source)?;
            for response in responses {
                writeln!(
                    f,
                    "{}",
                    response_formatter.format_detailed_response(response)
                )?;
            }

            if let Some(suggestions) = &stats.suggestion_map.get(source) {
                writeln!(f, "\n\u{2139} Suggestions")?;
                for suggestion in *suggestions {
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

        Ok(())
    }
}

fn write_if_any(
    value: usize,
    symbol: &str,
    text: &str,
    style: &Lazy<Style>,
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
    fn format(&self, stats: ResponseStats) -> Result<Option<String>> {
        let compact = CompactResponseStats {
            stats,
            mode: self.mode.clone(),
        };
        Ok(Some(compact.to_string()))
    }
}
