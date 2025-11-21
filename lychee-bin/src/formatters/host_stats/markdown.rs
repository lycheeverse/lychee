use std::{
    collections::HashMap,
    fmt::{self, Display},
};

use super::HostStatsFormatter;
use anyhow::Result;
use lychee_lib::ratelimit::HostStats;
use tabled::{
    Table, Tabled,
    settings::{Alignment, Modify, Style, object::Segment},
};

#[derive(Tabled)]
struct HostStatsTableEntry {
    #[tabled(rename = "Host")]
    host: String,
    #[tabled(rename = "Requests")]
    requests: u64,
    #[tabled(rename = "Success Rate")]
    success_rate: String,
    #[tabled(rename = "Median Time")]
    median_time: String,
    #[tabled(rename = "Cache Hit Rate")]
    cache_hit_rate: String,
}

fn host_stats_table(host_stats: &HashMap<String, HostStats>) -> String {
    let sorted_hosts = super::sort_host_stats(host_stats);

    let entries: Vec<HostStatsTableEntry> = sorted_hosts
        .into_iter()
        .map(|(hostname, stats)| {
            let median_time = stats
                .median_request_time()
                .map_or_else(|| "N/A".to_string(), |d| format!("{:.0}ms", d.as_millis()));

            HostStatsTableEntry {
                host: hostname.clone(),
                requests: stats.total_requests,
                success_rate: format!("{:.1}%", stats.success_rate() * 100.0),
                median_time,
                cache_hit_rate: format!("{:.1}%", stats.cache_hit_rate() * 100.0),
            }
        })
        .collect();

    if entries.is_empty() {
        return String::new();
    }

    let style = Style::markdown();
    Table::new(entries)
        .with(Modify::new(Segment::all()).with(Alignment::left()))
        .with(style)
        .to_string()
}

struct MarkdownHostStats(HashMap<String, HostStats>);

impl Display for MarkdownHostStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return Ok(());
        }

        writeln!(f, "\n## Per-host Statistics")?;
        writeln!(f)?;
        writeln!(f, "{}", host_stats_table(&self.0))?;

        Ok(())
    }
}

pub(crate) struct Markdown;

impl Markdown {
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl HostStatsFormatter for Markdown {
    fn format(&self, host_stats: HashMap<String, HostStats>) -> Result<Option<String>> {
        if host_stats.is_empty() {
            return Ok(None);
        }

        let markdown = MarkdownHostStats(host_stats);
        Ok(Some(markdown.to_string()))
    }
}
