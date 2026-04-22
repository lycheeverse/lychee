//! Provide the means to display practical user-friendly messages.

use std::{fmt::Display, sync::Mutex};

use lychee_lib::StatusCodeSelector;

use crate::{config::Config, formatters::stats::ResponseStats};

/// Hints are accumulated during a single program invocation.
static HINTS: Mutex<Vec<Hint>> = Mutex::new(vec![]);

/// A informative and friendly message created during the invocation of the program
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

pub(crate) fn show_hints() {
    for hint in HINTS.lock().unwrap().iter() {
        eprintln!("Hint: {hint}");
    }
}

/// Collect hints based on the resulting statistics.
pub(crate) fn handle_stats(stats: &ResponseStats, config: &Config) {
    github_token(stats, config);
    any_redirects(stats, config);
    rejected_status_codes(stats, config);
    unfollowed_redirects(stats, config);
}

fn github_token(stats: &ResponseStats, config: &Config) {
    const MESSAGE: &str = "There were issues with GitHub URLs. \
    You could try setting a GitHub token with --github-token";

    let any_github_errors = stats
        .error_map
        .values()
        .flatten()
        .any(|body| body.uri.domain() == Some("github.com"));

    if config.github_token.is_none() && any_github_errors {
        add_hint(MESSAGE.into());
    }
}

fn any_redirects(stats: &ResponseStats, config: &Config) {
    const DETAILS: &str = "You might want to consider replacing redirecting URLs with the resolved URLs. \
    Use verbose mode (-vv) to see redirection details.";

    let count = stats.redirects;
    let has_redirects = count > 0;
    let hides_redirects = config.verbose().log_level() < log::Level::Info;

    if has_redirects && hides_redirects {
        let noun = if count == 1 { "redirect" } else { "redirects" };
        add_hint(format!("Followed {count} {noun}. {DETAILS}").into());
    }
}

fn rejected_status_codes(stats: &ResponseStats, config: &Config) {
    const MESSAGE: &str = "You can configure accepted response codes with -a or --accept";

    let is_default = config.accept() == StatusCodeSelector::default_accepted();
    let any_rejected_codes = stats.error_map.values().any(|v| {
        v.iter().any(|v| {
            matches!(
                v.status,
                lychee_lib::Status::Error(lychee_lib::ErrorKind::RejectedStatusCode(_))
            )
        })
    });

    if is_default && any_rejected_codes {
        add_hint(MESSAGE.into());
    }
}

fn unfollowed_redirects(stats: &ResponseStats, config: &Config) {
    const MESSAGE: &str = "Rejected redirecional status codes. \
    This means some redirects were not followed. \
    You might want to increase the limit for -m/--max-redirects.";

    let is_small_limit = config.max_redirects() <= lychee_lib::DEFAULT_MAX_REDIRECTS;
    let any_rejected_redirection_codes = stats.error_map.values().any(|v| {
        v.iter().any(|v| {
            matches!(
                v.status,
                lychee_lib::Status::Error(lychee_lib::ErrorKind::RejectedStatusCode(s)) if s.is_redirection()
            )
        })
    });

    if is_small_limit && any_rejected_redirection_codes {
        add_hint(MESSAGE.into());
    }
}
