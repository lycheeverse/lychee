use super::StatsFormatter;
use crate::{
    config,
    formatters::{
        get_response_formatter,
        host_stats::DetailedHostStats,
        stats::{OutputStats, ResponseStats},
    },
};

use anyhow::Result;
use lychee_lib::InputSource;
use std::{
    collections::HashSet,
    fmt::{self, Display},
};
// Maximum padding for each entry in the final statistics output
const WIDTH: usize = 20;

fn write_stat(f: &mut fmt::Formatter, title: &str, stat: usize, newline: bool) -> fmt::Result {
    f.write_str(title)?;

    let spacing = WIDTH.saturating_sub(title.chars().count());
    f.write_str(format!("{stat:.>spacing$}").as_str())?;

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
    mode: config::OutputMode,
}

impl Display for DetailedResponseStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stats = &self.stats;
        let separator = "-".repeat(WIDTH + 1);

        writeln!(f, "📝 Summary")?;
        writeln!(f, "{separator}")?;
        write_stat(f, "🔍 Total", stats.total, true)?;
        write_stat(f, "🔗 Unique", stats.unique, true)?;
        write_stat(f, "✅ Successful", stats.successful, true)?;
        write_stat(f, "⏳ Timeouts", stats.timeouts, true)?;
        write_stat(f, "🔀 Redirected", stats.redirects, true)?;
        write_stat(f, "👻 Excluded", stats.excludes, true)?;
        write_stat(f, "❓ Unknown", stats.unknown, true)?;
        write_stat(f, "🚫 Errors", stats.errors, true)?;
        write_stat(f, "⛔ Unsupported", stats.unsupported, false)?;

        let response_formatter = get_response_formatter(&self.mode);

        for (source, responses) in
            super::sort_stats_iter(stats.error_map.iter().chain(stats.timeout_map.iter()))
        {
            // Using leading newlines over trailing ones (e.g. `writeln!`)
            // lets us avoid extra newlines without any additional logic.
            write!(f, "\n\nErrors in {source}")?;

            for response in responses {
                write!(f, "\n{}", response_formatter.format_response(response))?;
            }

            write_stats(f, "Suggestions", source, stats.suggestion_map.get(source))?;
            write_stats(f, "Redirects", source, stats.redirect_map.get(source))?;
        }

        // Ignored (unsupported) links get their own section so they are not
        // mislabeled as errors, and so inputs whose only finding is an ignored
        // link are still listed.
        for (source, responses) in super::sort_stats_iter(stats.unsupported_map.iter()) {
            write!(f, "\n\nIgnored in {source}")?;

            for response in responses {
                write!(f, "\n{}", response_formatter.format_response(response))?;
            }
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
    mode: config::OutputMode,
}

impl Detailed {
    pub(crate) const fn new(mode: config::OutputMode) -> Self {
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
    use crate::{config::OutputMode, formatters::stats::get_dummy_stats};
    use pretty_assertions::assert_eq;

    #[test]
    fn test_detailed_formatter() {
        let formatter = Detailed::new(OutputMode::Plain);
        let result = formatter.format(get_dummy_stats()).unwrap();

        assert_eq!(
            result,
            "📝 Summary
---------------------
🔍 Total............2
🔗 Unique...........2
✅ Successful.......0
⏳ Timeouts.........1
🔀 Redirected.......1
👻 Excluded.........0
❓ Unknown..........0
🚫 Errors...........1
⛔ Unsupported......0

Errors in https://example.com/
[404] https://github.com/mre/idiomatic-rust-doesnt-exist-man (at 1:1) | 404 Not Found
[TIMEOUT] https://httpbin.org/delay/2 (at 1:1) | Request timed out

Suggestions in https://example.com/
https://original.dev/ --> https://suggestion.dev/


Redirects in https://example.com/
https://1.dev/ --[308]--> https://2.dev/ --[308]--> http://redirected.dev/


📊 Per-host Statistics (1 domains, 5 links checked)
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

    #[test]
    fn test_detailed_formatter_lists_ignored_only_input() {
        use std::collections::{HashMap, HashSet};
        use std::time::Duration;

        use lychee_lib::{ErrorKind, InputSource, ResponseBody, Status, Uri};
        use url::Url;

        // An input whose only problem is an ignored link should get its own
        // "Ignored in <source>" section, not be mislabeled under "Errors".
        let source = InputSource::RemoteUrl(Box::new(Url::parse("https://example.com").unwrap()));
        let unsupported_map = HashMap::from([(
            source,
            HashSet::from([ResponseBody {
                uri: Uri::try_from("https://example.com/ignored").unwrap(),
                status: Status::Unsupported(ErrorKind::InvalidUrlHost),
                redirects: None,
                remap: None,
                span: None,
                duration: Some(Duration::from_secs(1)),
            }]),
        )]);

        let stats = ResponseStats {
            total: 1,
            unique: 1,
            unsupported: 1,
            unsupported_map,
            ..Default::default()
        };

        let response_stats = DetailedResponseStats {
            stats,
            mode: OutputMode::Plain,
        };

        assert_eq!(
            response_stats.to_string(),
            "📝 Summary
---------------------
🔍 Total............1
🔗 Unique...........1
✅ Successful.......0
⏳ Timeouts.........0
🔀 Redirected.......0
👻 Excluded.........0
❓ Unknown..........0
🚫 Errors...........0
⛔ Unsupported......1

Ignored in https://example.com/
[IGNORED] https://example.com/ignored | Unsupported: URL is missing a hostname"
        );
    }
}
