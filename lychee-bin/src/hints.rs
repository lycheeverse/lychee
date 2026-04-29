use http::StatusCode;
use lychee_lib::{ErrorKind, Status, StatusCodeSelector, hint};

use crate::{config::Config, formatters::stats::ResponseStats};

/// Collect [`lychee_lib::Hint`]s based on the resulting statistics.
pub(crate) fn handle_stats(stats: &ResponseStats, config: &Config) {
    rate_limit(stats, config);
    github_rate_limit(stats, config);
    any_redirects(stats, config);
    rejected_status_codes(stats, config);
    unfollowed_redirects(stats, config);
}

fn rate_limit(stats: &ResponseStats, config: &Config) {
    let default_host_config = config.hosts.is_empty();
    let first_rate_limited_domain = stats
        .error_map
        .values()
        .flatten()
        .find(|r| {
            matches!(
                r.status,
                Status::Error(ErrorKind::RejectedStatusCode(StatusCode::TOO_MANY_REQUESTS))
            )
        })
        .and_then(|b| b.uri.domain());

    if default_host_config && let Some(domain) = first_rate_limited_domain {
        hint!(
            "Encountered rate limit responses. \
            You might be able to work around this by adding `[hosts.\"{domain}\"]` to the TOML config \
            to adjust the `concurrency` and `request_interval` values."
        );
    }
}

/// Github rate limits can be circumvented by specifying a token.
/// According to the [docs]:
///
/// > If you exceed your primary rate limit, you will receive a 403 or 429 response
///
/// [docs]: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api?apiVersion=2026-03-10#exceeding-the-rate-limit
fn github_rate_limit(stats: &ResponseStats, config: &Config) {
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
        hint!(
            "GitHub seems to be rate limiting us. \
             You could try setting a GitHub token with `--github-token`"
        );
    }
}

fn any_redirects(stats: &ResponseStats, config: &Config) {
    let count = stats.redirects;
    let has_redirects = count > 0;
    let hides_redirects = config.verbose().log_level() < log::Level::Info;

    if has_redirects && hides_redirects {
        let noun = if count == 1 { "redirect" } else { "redirects" };
        hint!(
            "Followed {count} {noun}. \
             You might want to consider replacing redirecting URLs with the resolved URLs. \
             Use verbose mode (`-v`/`-vv`) to see redirection details."
        );
    }
}

fn rejected_status_codes(stats: &ResponseStats, config: &Config) {
    let is_default = config.accept() == StatusCodeSelector::default_accepted();
    let any_rejected_codes = stats
        .error_map
        .values()
        .flatten()
        .any(|r| matches!(r.status, Status::Error(ErrorKind::RejectedStatusCode(_))));

    if is_default && any_rejected_codes {
        hint!("You can configure accepted/rejected response codes with `-a` or `--accept`");
    }
}

fn unfollowed_redirects(stats: &ResponseStats, config: &Config) {
    let is_small_limit = config.max_redirects() <= lychee_lib::DEFAULT_MAX_REDIRECTS;
    let any_rejected_redirection_codes = stats.error_map.values().flatten().any(|r| {
        matches!(
            r.status,
            Status::Error(ErrorKind::RejectedStatusCode(s)) if s.is_redirection()
        )
    });

    if is_small_limit && any_rejected_redirection_codes {
        hint!(
            "Rejected redirectional status codes. \
             This means some redirects were not followed. \
             You might want to increase the limit for `-m`/`--max-redirects`."
        );
    }
}
