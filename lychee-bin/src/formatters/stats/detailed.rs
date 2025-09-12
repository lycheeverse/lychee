use super::StatsFormatter;
use crate::{formatters::get_response_formatter, options, stats::ResponseStats};

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
                write!(
                    f,
                    "\n{}",
                    response_formatter.format_detailed_response(response)
                )?;
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
    use lychee_lib::{InputSource, Redirects, ResponseBody, Status};
    use std::collections::{HashMap, HashSet};
    use url::Url;

    #[test]
    fn test_detailed_formatter() {
        let source = InputSource::RemoteUrl(Box::new(Url::parse("https://example.com").unwrap()));
        let error_map = HashMap::from([(
            source.clone(),
            HashSet::from([
                ResponseBody {
                    uri: "https://github.com/mre/idiomatic-rust-doesnt-exist-man"
                        .try_into()
                        .unwrap(),
                    status: Status::Ok(StatusCode::NOT_FOUND),
                },
                ResponseBody {
                    uri: "https://github.com/mre/boom".try_into().unwrap(),
                    status: Status::Ok(StatusCode::INTERNAL_SERVER_ERROR),
                },
            ]),
        )]);

        let suggestion_map = HashMap::from([(
            source.clone(),
            HashSet::from([Suggestion {
                original: "https://original.dev".try_into().unwrap(),
                suggestion: "https://suggestion.dev".try_into().unwrap(),
            }]),
        )]);

        let redirect_map = HashMap::from([(
            source,
            HashSet::from([ResponseBody {
                uri: "https://redirected.dev".try_into().unwrap(),
                status: Status::Redirected(
                    StatusCode::OK,
                    Redirects::from(vec![
                        Url::parse("https://1.dev").unwrap(),
                        Url::parse("https://2.dev").unwrap(),
                        Url::parse("http://redirected.dev").unwrap(),
                    ]),
                ),
            }]),
        )]);

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
            redirect_map,
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


Redirects in https://example.com/
https://redirected.dev/ | Redirect: Followed 2 redirects resolving to the final status of: OK. Redirects: https://1.dev/ --> https://2.dev/ --> http://redirected.dev/
"
        );
    }
}
