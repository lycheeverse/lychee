use super::StatsFormatter;
use crate::{formatters::get_response_formatter, options, stats::ResponseStats};

use anyhow::Result;
use pad::{Alignment, PadStr};
use std::fmt::{self, Display};
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
                write!(
                    f,
                    "\n{}",
                    response_formatter.format_detailed_response(response)
                )?;
            }

            if let Some(suggestions) = stats.suggestion_map.get(source) {
                // Sort suggestions
                let mut sorted_suggestions: Vec<_> = suggestions.iter().collect();
                sorted_suggestions.sort_by(|a, b| {
                    let (a, b) = (a.to_string().to_lowercase(), b.to_string().to_lowercase());
                    numeric_sort::cmp(&a, &b)
                });

                writeln!(f, "\n\nSuggestions in {source}")?;
                for suggestion in sorted_suggestions {
                    writeln!(f, "{suggestion}")?;
                }
            }
        }

        Ok(())
    }
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
    fn format(&self, stats: ResponseStats) -> Result<Option<String>> {
        let detailed = DetailedResponseStats {
            stats,
            mode: self.mode.clone(),
        };
        Ok(Some(detailed.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{formatters::suggestion::Suggestion, options::OutputMode};
    use http::StatusCode;
    use lychee_lib::{InputSource, ResponseBody, Status, Uri};
    use std::collections::{HashMap, HashSet};
    use url::Url;

    #[test]
    fn test_detailed_formatter() {
        let err1 = ResponseBody {
            uri: Uri::try_from("https://github.com/mre/idiomatic-rust-doesnt-exist-man").unwrap(),
            status: Status::Ok(StatusCode::NOT_FOUND),
        };

        let err2 = ResponseBody {
            uri: Uri::try_from("https://github.com/mre/boom").unwrap(),
            status: Status::Ok(StatusCode::INTERNAL_SERVER_ERROR),
        };

        let mut error_map = HashMap::new();
        let source = InputSource::RemoteUrl(Box::new(Url::parse("https://example.com").unwrap()));
        error_map.insert(source.clone(), HashSet::from_iter(vec![err1, err2]));

        let mut suggestion_map = HashMap::new();
        suggestion_map.insert(
            source,
            HashSet::from_iter(vec![Suggestion {
                original: "https://original.dev".try_into().unwrap(),
                suggestion: "https://suggestion.dev".try_into().unwrap(),
            }]),
        );

        let stats = ResponseStats {
            total: 2,
            successful: 0,
            errors: 2,
            unknown: 0,
            excludes: 0,
            timeouts: 0,
            duration_secs: 0,
            unsupported: 0,
            redirects: 0,
            cached: 0,
            suggestion_map,
            redirect_map: HashMap::default(),
            success_map: HashMap::default(),
            error_map,
            excluded_map: HashMap::default(),
            detailed_stats: true,
        };

        let formatter = Detailed::new(OutputMode::Plain);
        let result = formatter.format(stats).unwrap().unwrap();

        assert_eq!(
            result,
            "📝 Summary
---------------------
🔍 Total............2
✅ Successful.......0
⏳ Timeouts.........0
🔀 Redirected.......0
👻 Excluded.........0
❓ Unknown..........0
🚫 Errors...........2
⛔ Unsupported......2

Errors in https://example.com/
[500] https://github.com/mre/boom | 500 Internal Server Error: Internal Server Error
[404] https://github.com/mre/idiomatic-rust-doesnt-exist-man | 404 Not Found: Not Found

Suggestions in https://example.com/
https://original.dev/ --> https://suggestion.dev/
"
        );
    }
}
