use super::StatsFormatter;
use crate::{
    formatters::{
        get_response_formatter,
        host_stats::DetailedHostStats,
        stats::{OutputStats, ResponseStats},
    },
    options,
};

use anyhow::Result;
use lychee_lib::InputSource;
use pad::{Alignment, PadStr};
use std::{
    collections::HashSet,
    fmt::{self, Display},
};
// Maximum padding for each entry in the final statistics output
const MAX_PADDING: usize = 20;

fn write_stat(f: &mut fmt::Formatter, title: &str, stat: usize, newline: bool) -> fmt::Result {
    let fill = title.chars().count();
    f.write_str(title)?;
    f.write_str(
        &stat
            .to_string()
            .pad(MAX_PADDING - fill, '.', Alignment::Right, false),
    )?;

    if newline {
        f.write_str("\n")?;
    }

    Ok(())
}

/// A wrapper struct that combines `ResponseStats` with an additional `OutputMode`.
/// Multiple `Display` implementations are not allowed for `ResponseStats`, so this struct is used to
/// encapsulate additional context.
struct DetailedResponseStats {
    stats: ResponseStats,
    mode: options::OutputMode,
}

impl Display for DetailedResponseStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stats = &self.stats;
        let separator = "-".repeat(MAX_PADDING + 1);

        writeln!(f, "📝 Summary")?;
        writeln!(f, "{separator}")?;
        write_stat(f, "🔍 Total", stats.total, true)?;
        write_stat(f, "✅ Successful", stats.successful, true)?;
        write_stat(f, "⏳ Timeouts", stats.timeouts, true)?;
        write_stat(f, "🔀 Redirected", stats.redirects, true)?;
        write_stat(f, "👻 Excluded", stats.excludes, true)?;
        write_stat(f, "❓ Unknown", stats.unknown, true)?;
        write_stat(f, "🚫 Errors", stats.errors, true)?;
        write_stat(f, "⛔ Unsupported", stats.errors, false)?;

        let response_formatter = get_response_formatter(&self.mode);

        for (source, responses) in super::sort_stat_map(&stats.error_map) {
            // Using leading newlines over trailing ones (e.g. `writeln!`)
            // lets us avoid extra newlines without any additional logic.
            write!(f, "\n\nErrors in {source}")?;

            for response in responses {
                write!(f, "\n{}", response_formatter.format_response(response))?;
            }

            write_stats(f, "Suggestions", source, stats.suggestion_map.get(source))?;
            write_stats(f, "Redirects", source, stats.redirect_map.get(source))?;
        }

        Ok(())
    }
}

fn write_stats<T: Display>(
    f: &mut fmt::Formatter<'_>,
    title: &str,
    source: &InputSource,
    set: Option<&HashSet<T>>,
) -> Result<(), fmt::Error> {
    if let Some(items) = set {
        let mut sorted: Vec<_> = items.iter().collect();
        sorted.sort_by(|a, b| {
            let (a, b) = (a.to_string().to_lowercase(), b.to_string().to_lowercase());
            numeric_sort::cmp(&a, &b)
        });

        writeln!(f, "\n\n{title} in {source}")?;
        for item in sorted {
            writeln!(f, "{item}")?;
        }
    }
    Ok(())
}

pub(crate) struct Detailed {
    mode: options::OutputMode,
}

impl Detailed {
    pub(crate) const fn new(mode: options::OutputMode) -> Self {
        Self { mode }
    }
}

impl StatsFormatter for Detailed {
    fn format(&self, stats: OutputStats) -> Result<String> {
        let response_stats = DetailedResponseStats {
            stats: stats.response_stats,
            mode: self.mode.clone(),
        };
        let host_stats = DetailedHostStats {
            host_stats: stats.host_stats,
        };

        Ok(format!("{response_stats}\n{host_stats}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{formatters::stats::get_dummy_stats, options::OutputMode};

    #[test]
    fn test_detailed_formatter() {
        let formatter = Detailed::new(OutputMode::Plain);
        let result = formatter.format(get_dummy_stats()).unwrap();

        assert_eq!(
            result,
            "📝 Summary
---------------------
🔍 Total............2
✅ Successful.......0
⏳ Timeouts.........0
🔀 Redirected.......1
👻 Excluded.........0
❓ Unknown..........0
🚫 Errors...........1
⛔ Unsupported......1

Errors in https://example.com/
[404] https://github.com/mre/idiomatic-rust-doesnt-exist-man | 404 Not Found: Not Found

Suggestions in https://example.com/
https://original.dev/ --> https://suggestion.dev/


Redirects in https://example.com/
https://redirected.dev/ | Redirect: Followed 2 redirects resolving to the final status of: OK. Redirects: https://1.dev/ --> https://2.dev/ --> http://redirected.dev/


📊 Per-host Statistics
---------------------

Host: example.com
  Total requests: 5
  Successful: 3 (60.0%)
  Rate limited: 1 (429 Too Many Requests)
  Server errors (5xx): 1
  Cache hit rate: 20.0%
  Cache hits: 1, misses: 4
"
        );
    }
}
