use std::fmt;

use lychee_lib::ratelimit::HostStatsMap;

mod compact;
mod detailed;
mod markdown;

pub(crate) use compact::CompactHostStats;
pub(crate) use detailed::DetailedHostStats;
pub(crate) use markdown::MarkdownHostStats;

/// Writes the heading for a host statistics section.
fn write_host_heading(
    f: &mut fmt::Formatter<'_>,
    prefix: &str,
    host_stats: &HostStatsMap,
) -> fmt::Result {
    writeln!(
        f,
        "{prefix}Per-host Statistics ({hosts} domains, {requests} requests)",
        hosts = host_stats.total_hosts(),
        requests = host_stats.total_requests(),
    )
}
