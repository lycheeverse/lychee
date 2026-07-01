use std::fmt::{self, Display};

use crate::formatters::color::{DIM, NORMAL, color};
use lychee_lib::ratelimit::HostStatsMap;

pub(crate) struct CompactHostStats {
    pub(crate) host_stats: Option<HostStatsMap>,
}

impl Display for CompactHostStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(host_stats) = &self.host_stats else {
            return Ok(());
        };

        writeln!(f)?;
        let sorted_hosts = host_stats.sorted();
        let domain_count = sorted_hosts.len();
        let total_links: u64 = sorted_hosts.iter().map(|(_, s)| s.total_requests).sum();
        writeln!(
            f,
            "📊 Per-host Statistics ({domain_count} domains, {total_links} links checked)"
        )?;

        let separator = "─".repeat(60);
        color!(f, DIM, "{}", separator)?;
        writeln!(f)?;

        // Calculate optimal hostname width based on longest hostname
        let max_hostname_len = sorted_hosts
            .iter()
            .map(|(hostname, _)| hostname.len())
            .max()
            .unwrap_or(0);
        let hostname_width = (max_hostname_len + 2).max(10); // At least 10 chars with padding

        for (hostname, stats) in sorted_hosts {
            let median_time = stats
                .median_request_time()
                .map_or_else(|| "N/A".to_string(), |d| format!("{:.0}ms", d.as_millis()));

            let cache_hit_rate = stats.cache_hit_rate() * 100.0;

            color!(
                f,
                NORMAL,
                "{:<width$} │ {:>6} reqs │ {:>6.1}% success │ {:>8} median │ {:>6.1}% cached",
                hostname,
                stats.total_requests,
                stats.success_rate() * 100.0,
                median_time,
                cache_hit_rate,
                width = hostname_width
            )?;
            writeln!(f)?;
        }

        Ok(())
    }
}
