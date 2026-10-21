use std::fmt::{self, Write};

use super::{STATS_EMOJI, host_heading, status_summary};
use crate::config::OutputMode;
use crate::formatters::color::{NORMAL, color};
use lychee_lib::ratelimit::HostStatsMap;

pub(crate) struct CompactHostStats {
    pub(crate) host_stats: Option<HostStatsMap>,
}

impl CompactHostStats {
    pub(crate) fn format(&self, mode: &OutputMode) -> Result<String, fmt::Error> {
        let mut buf = String::new();
        let Some(host_stats) = &self.host_stats else {
            return Ok(buf);
        };

        buf.push('\n');
        if *mode != OutputMode::Plain {
            buf.push_str(STATS_EMOJI);
        }
        writeln!(buf, "{}", host_heading(host_stats))?;

        let sorted_hosts = host_stats.sorted();
        let hostname_width = sorted_hosts
            .iter()
            .map(|(hostname, _)| hostname.len())
            .max()
            .unwrap_or(0)
            .max(10);

        for (hostname, stats) in sorted_hosts {
            let status_summary = status_summary(&stats);
            let cache_summary = stats.cache_summary();

            color!(
                buf,
                NORMAL,
                "  {hostname:<width$}  {:>6} reqs  {cache_summary:>12}    {status_summary}",
                stats.total_requests,
                width = hostname_width,
            )?;
            writeln!(buf)?;
        }

        Ok(buf)
    }
}
