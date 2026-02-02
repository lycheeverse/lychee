//! Display practical user-friendly hints
//! after the link checking process is done.

use log::warn;

use crate::{formatters::stats::ResponseStats, options::Config};

const GITHUB_ERRORS: &str = "There were issues with GitHub URLs. You could try setting a GitHub token and running lychee again.";

pub(crate) fn display_hints(stats: &ResponseStats, config: &Config) {
    github_warning(stats, config);
    redirect_warning(stats, config);
    rejected_status_code_warning(stats);
}

/// Display user-friendly message if there were any issues with GitHub URLs
fn github_warning(stats: &ResponseStats, config: &Config) {
    let github_errors = stats
        .error_map
        .values()
        .flatten()
        .any(|body| body.uri.domain() == Some("github.com"));

    if github_errors && config.github_token.is_none() {
        show_hint(GITHUB_ERRORS);
    }
}

/// Display user-friendly message if there were any redirects
/// in non-verbose mode.
fn redirect_warning(stats: &ResponseStats, config: &Config) {
    let redirects = stats.redirects;
    if redirects > 0 && config.verbose.log_level() < log::Level::Info {
        let noun = if redirects == 1 {
            "redirect"
        } else {
            "redirects"
        };

        show_hint(&format!(
            "lychee detected {redirects} {noun}. You might want to consider replacing redirecting URLs with the resolved URLs. Run lychee in verbose mode (-v/--verbose) to see details about the redirections."
        ));
    }
}

/// Display user-friendly message if there were any
/// rejected status codes.
fn rejected_status_code_warning(stats: &ResponseStats) {
    let any_rejected_codes = stats.error_map.values().any(|v| {
        v.iter().any(|v| {
            matches!(
                v.status,
                lychee_lib::Status::Error(lychee_lib::ErrorKind::RejectedStatusCode(_))
            )
        })
    });

    if any_rejected_codes {
        // TODO: create `Hint` struct?
        show_hint(r#"You can configure accepted response codes with the "accept" option"."#);
    }
}

fn show_hint(message: &str) {
    // TODO: log this after the summary & results at the bottom for better UX ?
    eprintln!("{}", message);
}
