use std::fmt::{self, Display};

use super::{status_summary, write_host_heading};
use crate::formatters::color::{NORMAL, color};
use lychee_lib::ratelimit::HostStatsMap;

pub(crate) struct CompactHostStats {
    pub(crate) host_stats: Option<HostStatsMap>,
}

impl Display for CompactHostStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(host_stats) = &self.host_stats else {
            return Ok(());
        };

        write_host_heading(f, "\n📊 ", host_stats)?;

        let sorted_hosts = host_stats.sorted();
        let hostname_width = sorted_hosts
            .iter()
            .map(|(hostname, _)| hostname.len())
            .max()
            .unwrap_or(0)
            .max(10);

        for (hostname, stats) in sorted_hosts {
            color!(
                f,
                NORMAL,
                "  {hostname:<width$}  {:>6} reqs  {}",
                stats.total_requests,
                status_summary(&stats),
                width = hostname_width,
            )?;
            writeln!(f)?;
        }

        Ok(())
    }
}
