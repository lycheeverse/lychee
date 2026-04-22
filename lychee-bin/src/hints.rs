//! Provide the means to display practical user-friendly messages.

use std::{fmt::Display, sync::Mutex};

use http::StatusCode;
use lychee_lib::{ErrorKind, Status, StatusCodeSelector};

use crate::{config::Config, formatters::stats::ResponseStats, verbosity::Verbosity};

/// Hints are accumulated during a single program invocation.
static HINTS: Mutex<Vec<Hint>> = Mutex::new(vec![]);

/// An informative and friendly message created during the invocation of the program
/// to be displayed before termination, to improve user experience.
pub(crate) struct Hint(String);

impl Display for Hint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Hint {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for Hint {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

pub(crate) fn add_hint(hint: Hint) {
    HINTS.lock().unwrap().push(hint);
}

pub(crate) fn show_hints(verbosity: &Verbosity) {
    if verbosity.log_level() > log::Level::Error {
        for hint in HINTS.lock().unwrap().iter() {
            eprintln!("Hint: {hint}");
        }
    }
}

/// Collect hints based on the resulting statistics.
pub(crate) fn handle_stats(stats: &ResponseStats, config: &Config) {
    github_rate_limit(stats, config);
    any_redirects(stats, config);
    rejected_status_codes(stats, config);
    unfollowed_redirects(stats, config);
}

/// Github rate limits can be circumvented by specifying a token.
/// According to the [docs]:
///
/// > If you exceed your primary rate limit, you will receive a 403 or 429 response
///
/// [docs]: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api?apiVersion=2026-03-10#exceeding-the-rate-limit
fn github_rate_limit(stats: &ResponseStats, config: &Config) {
    const MESSAGE: &str = "GitHub seems to be rate limiting us. \
    You could try setting a GitHub token with --github-token";

    let any_github_errors = stats.error_map.values().flatten().any(|body| {
        let is_github = body
            .uri
            .domain()
            .is_some_and(|domain| domain.ends_with("github.com"));

        let is_rate_limited = matches!(
            body.status.code(),
            Some(StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS)
        );

        is_github && is_rate_limited
    });

    if config.github_token.is_none() && any_github_errors {
        add_hint(MESSAGE.into());
    }
}

fn any_redirects(stats: &ResponseStats, config: &Config) {
    const DETAILS: &str = "You might want to consider replacing redirecting URLs with the resolved URLs. \
    Use verbose mode (-v/-vv) to see redirection details.";

    let count = stats.redirects;
    let has_redirects = count > 0;
    let hides_redirects = config.verbose().log_level() < log::Level::Info;

    if has_redirects && hides_redirects {
        let noun = if count == 1 { "redirect" } else { "redirects" };
        add_hint(format!("Followed {count} {noun}. {DETAILS}").into());
    }
}

fn rejected_status_codes(stats: &ResponseStats, config: &Config) {
    const MESSAGE: &str = "You can configure accepted/rejected response codes with -a or --accept";

    let is_default = config.accept() == StatusCodeSelector::default_accepted();
    let any_rejected_codes = stats
        .error_map
        .values()
        .flatten()
        .any(|r| matches!(r.status, Status::Error(ErrorKind::RejectedStatusCode(_))));

    if is_default && any_rejected_codes {
        add_hint(MESSAGE.into());
    }
}

fn unfollowed_redirects(stats: &ResponseStats, config: &Config) {
    const MESSAGE: &str = "Rejected redirectional status codes. \
    This means some redirects were not followed. \
    You might want to increase the limit for -m/--max-redirects.";

    let is_small_limit = config.max_redirects() <= lychee_lib::DEFAULT_MAX_REDIRECTS;
    let any_rejected_redirection_codes = stats.error_map.values().any(|v| {
        v.iter().any(|v| {
            matches!(
                v.status,
                Status::Error(ErrorKind::RejectedStatusCode(s)) if s.is_redirection()
            )
        })
    });

    if is_small_limit && any_rejected_redirection_codes {
        add_hint(MESSAGE.into());
    }
}
