use lychee_lib::ratelimit::{HostStats, HostStatsMap};

mod compact;
mod detailed;
mod markdown;

pub(crate) use compact::CompactHostStats;
pub(crate) use detailed::DetailedHostStats;
pub(crate) use markdown::MarkdownHostStats;

fn host_heading(prefix: &str, host_stats: &HostStatsMap) -> String {
    format!(
        "{prefix}Per-host Statistics ({hosts} domains, {requests} requests)",
        hosts = host_stats.total_hosts(),
        requests = host_stats.total_requests(),
    )
}

/// Returns a compact representation of the number of successful, failed, and
/// unknown requests.
///
/// # Example output
///
/// `[✓ 65, ✗ 2, ? 1]`
/// where
/// - `✓` indicates successful requests,
/// - `✗` indicates failed requests,
/// - `?` indicates unknown requests.
fn status_summary(stats: &HostStats) -> String {
    let failed_requests = stats.rate_limited + stats.client_errors + stats.server_errors;
    let unknown_requests = stats
        .total_requests
        .saturating_sub(stats.successful_requests + failed_requests);

    let mut parts = Vec::new();
    if stats.successful_requests > 0 {
        parts.push(format!("✓ {}", stats.successful_requests));
    }
    if failed_requests > 0 {
        parts.push(format!("✗ {failed_requests}"));
    }
    if unknown_requests > 0 {
        parts.push(format!("? {unknown_requests}"));
    }

    format!("[{}]", parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_summary_omits_zero_counts() {
        let stats = HostStats {
            total_requests: 68,
            successful_requests: 65,
            client_errors: 2,
            network_errors: 1,
            ..HostStats::default()
        };

        assert_eq!(status_summary(&stats), "[✓ 65, ✗ 2, ? 1]");
    }
}
