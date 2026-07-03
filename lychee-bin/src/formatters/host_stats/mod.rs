use std::fmt;

use lychee_lib::ratelimit::HostStatsMap;

mod compact;
mod detailed;
mod markdown;

pub(crate) use compact::CompactHostStats;
pub(crate) use detailed::DetailedHostStats;
pub(crate) use markdown::MarkdownHostStats;

/// Writes the header for a host statistics section.
fn write_header(
    f: &mut fmt::Formatter<'_>,
    prefix: &str,
    host_stats: &HostStatsMap,
) -> fmt::Result {
    // Host stats are appended after response stats,
    // so keep the section visually separated.
    writeln!(f)?;

    writeln!(
        f,
        "{prefix}Per-host Statistics ({hosts} domains, {requests} links checked)",
        hosts = host_stats.total_hosts(),
        requests = host_stats.total_requests(),
    )
}
