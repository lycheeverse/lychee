use anyhow::Result;
use std::{
    collections::HashMap,
    fmt::{self, Display},
};

use crate::formatters::color::{DIM, NORMAL, color};
use crate::options;
use lychee_lib::ratelimit::HostStats;

use super::HostStatsFormatter;

struct CompactHostStats {
    host_stats: HashMap<String, HostStats>,
}

impl Display for CompactHostStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host_stats.is_empty() {
            return Ok(());
        }

        writeln!(f)?;
        writeln!(f, "📊 Per-host Statistics")?;

        let separator = "─".repeat(60);
        color!(f, DIM, "{}", separator)?;
        writeln!(f)?;

        let sorted_hosts = super::sort_host_stats(&self.host_stats);

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
                "{:<width$} │ {:>6} reqs │ {:>6.1}% success │ {:>8} median │ {:>6.1}% cache",
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

pub(crate) struct Compact;

impl Compact {
    pub(crate) const fn new(_mode: options::OutputMode) -> Self {
        Self
    }
}

impl HostStatsFormatter for Compact {
    fn format(&self, host_stats: HashMap<String, HostStats>) -> Result<Option<String>> {
        if host_stats.is_empty() {
            return Ok(None);
        }

        let compact = CompactHostStats { host_stats };
        Ok(Some(compact.to_string()))
    }
}
