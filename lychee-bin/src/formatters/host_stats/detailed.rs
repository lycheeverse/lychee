use std::fmt::{self, Display};

use lychee_lib::ratelimit::HostStatsMap;

pub(crate) struct DetailedHostStats {
    pub(crate) host_stats: Option<HostStatsMap>,
}

impl Display for DetailedHostStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(host_stats) = &self.host_stats else {
            return Ok(());
        };

        writeln!(f, "\n📊 Per-host Statistics")?;
        writeln!(f, "---------------------")?;

        for (hostname, stats) in host_stats.sorted() {
            writeln!(f, "\nHost: {hostname}")?;
            writeln!(f, "  Total requests: {}", stats.total_requests)?;
            writeln!(
                f,
                "  Successful: {} ({:.1}%)",
                stats.successful_requests,
                stats.success_rate() * 100.0
            )?;

            if stats.rate_limited > 0 {
                writeln!(
                    f,
                    "  Rate limited: {} (429 Too Many Requests)",
                    stats.rate_limited
                )?;
            }
            if stats.client_errors > 0 {
                writeln!(f, "  Client errors (4xx): {}", stats.client_errors)?;
            }
            if stats.server_errors > 0 {
                writeln!(f, "  Server errors (5xx): {}", stats.server_errors)?;
            }

            if let Some(median_time) = stats.median_request_time() {
                writeln!(
                    f,
                    "  Median response time: {:.0}ms",
                    median_time.as_millis()
                )?;
            }

            let cache_hit_rate = stats.cache_hit_rate();
            if cache_hit_rate > 0.0 {
                writeln!(f, "  Cache hit rate: {:.1}%", cache_hit_rate * 100.0)?;
                writeln!(
                    f,
                    "  Cache hits: {}, misses: {}",
                    stats.cache_hits, stats.cache_misses
                )?;
            }
        }

        Ok(())
    }
}
