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
                    "[{}] {}",
                    response.status.code_as_string(),
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

#[cfg(test)]
mod tests {
    use crate::formatters::stats::StatsFormatter;
    use crate::{options::OutputMode, stats::ResponseStats};
    use http::StatusCode;
    use lychee_lib::{InputSource, ResponseBody, Status, Uri};
    use std::collections::{HashMap, HashSet};
    use url::Url;

    use super::*;

    #[test]
    fn test_formatter() {
        // A couple of dummy successes
        let mut success_map: HashMap<InputSource, HashSet<ResponseBody>> = HashMap::new();

        success_map.insert(
            InputSource::RemoteUrl(Box::new(Url::parse("https://example.com").unwrap())),
            HashSet::from_iter(vec![ResponseBody {
                uri: Uri::from(Url::parse("https://example.com").unwrap()),
                status: Status::Ok(StatusCode::OK),
            }]),
        );

        let err1 = ResponseBody {
            uri: Uri::try_from("https://github.com/mre/idiomatic-rust-doesnt-exist-man").unwrap(),
            status: Status::Ok(StatusCode::NOT_FOUND),
        };

        let err2 = ResponseBody {
            uri: Uri::try_from("https://github.com/mre/boom").unwrap(),
            status: Status::Ok(StatusCode::INTERNAL_SERVER_ERROR),
        };

        let mut fail_map: HashMap<InputSource, HashSet<ResponseBody>> = HashMap::new();
        let source = InputSource::RemoteUrl(Box::new(Url::parse("https://example.com").unwrap()));
        fail_map.insert(source, HashSet::from_iter(vec![err1, err2]));

        let stats = ResponseStats {
            total: 1,
            successful: 1,
            errors: 0,
            unknown: 0,
            excludes: 0,
            timeouts: 0,
            duration_secs: 0,
            fail_map,
            suggestion_map: HashMap::default(),
            unsupported: 0,
            redirects: 0,
            cached: 0,
            success_map,
            excluded_map: HashMap::default(),
            detailed_stats: false,
        };

        let formatter = Compact::new(OutputMode::Plain);

        let result = formatter.format(stats).unwrap().unwrap();

        println!("{result}");

        assert!(result.contains("🔍 1 Total"));
        assert!(result.contains("✅ 1 OK"));
        assert!(result.contains("🚫 0 Errors"));

        assert!(result.contains("[https://example.com/]:"));
        assert!(result
            .contains("https://github.com/mre/idiomatic-rust-doesnt-exist-man | 404 Not Found"));
        assert!(result.contains("https://github.com/mre/boom | 500 Internal Server Error"));
    }
}
