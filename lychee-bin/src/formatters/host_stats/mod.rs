mod compact;
mod detailed;
mod json;
mod markdown;

pub(crate) use compact::Compact;
pub(crate) use detailed::Detailed;
pub(crate) use json::Json;
pub(crate) use markdown::Markdown;

use anyhow::Result;
use lychee_lib::ratelimit::HostStats;
use std::collections::HashMap;

/// Trait for formatting per-host statistics in different output formats
pub(crate) trait HostStatsFormatter {
    /// Format the host statistics and return them as a string
    fn format(&self, host_stats: HashMap<String, HostStats>) -> Result<Option<String>>;
}

/// Sort host statistics by request count (descending order)
/// This matches the display order we want in the output
fn sort_host_stats(host_stats: &HashMap<String, HostStats>) -> Vec<(&String, &HostStats)> {
    let mut sorted_hosts: Vec<_> = host_stats.iter().collect();
    // Sort by total requests (descending)
    sorted_hosts.sort_by_key(|(_, stats)| std::cmp::Reverse(stats.total_requests));
    sorted_hosts
}
