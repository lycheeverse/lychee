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

        writeln!(f, "\u{1f4dd} Summary")?; // 📝
        writeln!(f, "{separator}")?;
        write_stat(f, "\u{1f50d} Total", stats.total, true)?; // 🔍
        write_stat(f, "\u{2705} Successful", stats.successful, true)?; // ✅
        write_stat(f, "\u{23f3} Timeouts", stats.timeouts, true)?; // ⏳
        write_stat(f, "\u{1f500} Redirected", stats.redirects, true)?; // 🔀
        write_stat(f, "\u{1f47b} Excluded", stats.excludes, true)?; // 👻
        write_stat(f, "\u{2753} Unknown", stats.unknown, true)?; //❓
        write_stat(f, "\u{1f6ab} Errors", stats.errors, false)?; // 🚫

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
                    human_sort::compare(&a, &b)
                });

                writeln!(f, "\nSuggestions in {source}")?;
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
    use crate::options::OutputMode;
    use http::StatusCode;
    use lychee_lib::{InputSource, ResponseBody, Status, Uri};
    use std::collections::{HashMap, HashSet};
    use url::Url;

    #[test]
    fn test_detailed_formatter_github_404() {
        let err1 = ResponseBody {
            uri: Uri::try_from("https://github.com/mre/idiomatic-rust-doesnt-exist-man").unwrap(),
            status: Status::Ok(StatusCode::NOT_FOUND),
        };

        let err2 = ResponseBody {
            uri: Uri::try_from("https://github.com/mre/boom").unwrap(),
            status: Status::Ok(StatusCode::INTERNAL_SERVER_ERROR),
        };

        let mut error_map: HashMap<InputSource, HashSet<ResponseBody>> = HashMap::new();
        let source = InputSource::RemoteUrl(Box::new(Url::parse("https://example.com").unwrap()));
        error_map.insert(source, HashSet::from_iter(vec![err1, err2]));

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
            suggestion_map: HashMap::default(),
            success_map: HashMap::default(),
            error_map,
            excluded_map: HashMap::default(),
            detailed_stats: true,
        };

        let formatter = Detailed::new(OutputMode::Plain);
        let result = formatter.format(stats).unwrap().unwrap();

        // Check for the presence of expected content
        assert!(result.contains("📝 Summary"));
        assert!(result.contains("🔍 Total............2"));
        assert!(result.contains("✅ Successful.......0"));
        assert!(result.contains("⏳ Timeouts.........0"));
        assert!(result.contains("🔀 Redirected.......0"));
        assert!(result.contains("👻 Excluded.........0"));
        assert!(result.contains("❓ Unknown..........0"));
        assert!(result.contains("🚫 Errors...........2"));
        assert!(result.contains("Errors in https://example.com/"));
        assert!(
            result
                .contains("https://github.com/mre/idiomatic-rust-doesnt-exist-man | 404 Not Found")
        );
        assert!(result.contains("https://github.com/mre/boom | 500 Internal Server Error"));
    }
}
