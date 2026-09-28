use std::fmt::{self, Write};

use super::{STATS_EMOJI, host_heading};
use crate::config::OutputMode;
use lychee_lib::ratelimit::HostStatsMap;

pub(crate) struct DetailedHostStats {
    pub(crate) host_stats: Option<HostStatsMap>,
}

impl DetailedHostStats {
    pub(crate) fn format(&self, mode: &OutputMode) -> Result<String, fmt::Error> {
        let mut buf = String::new();
        let Some(host_stats) = &self.host_stats else {
            return Ok(buf);
        };

        let mut heading = String::new();
        if *mode != OutputMode::Plain {
            heading.push_str(STATS_EMOJI);
        }
        heading.push_str(&host_heading(host_stats));
        writeln!(buf, "\n{heading}")?;
        writeln!(buf, "{}", "-".repeat(heading.chars().count()))?;

        for (hostname, stats) in host_stats.sorted() {
            writeln!(buf, "\nHost: {hostname}")?;
            writeln!(buf, "  Total requests: {}", stats.total_requests)?;
            writeln!(
                buf,
                "  Successful: {} ({:.1}%)",
                stats.successful_requests,
                stats.success_rate() * 100.0
            )?;

            if stats.rate_limited > 0 {
                writeln!(
                    buf,
                    "  Rate limited: {} (429 Too Many Requests)",
                    stats.rate_limited
                )?;
            }
            if stats.client_errors > 0 {
                writeln!(buf, "  Client errors (4xx): {}", stats.client_errors)?;
            }
            if stats.server_errors > 0 {
                writeln!(buf, "  Server errors (5xx): {}", stats.server_errors)?;
            }
            if stats.network_errors > 0 {
                writeln!(buf, "  Network errors: {}", stats.network_errors)?;
            }

            if let Some(median_time) = stats.median_request_time() {
                writeln!(
                    buf,
                    "  Median response time: {:.0}ms",
                    median_time.as_millis()
                )?;
            }

            let cache_hit_rate = stats.cache_hit_rate();
            if cache_hit_rate > 0.0 {
                writeln!(buf, "  Cache hit rate: {:.1}%", cache_hit_rate * 100.0)?;
                writeln!(
                    buf,
                    "  Cache hits: {}, misses: {}",
                    stats.cache_hits, stats.cache_misses
                )?;
            }
        }

        Ok(buf)
    }
}
