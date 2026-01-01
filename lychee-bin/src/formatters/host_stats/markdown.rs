use std::fmt::{self, Display};

use lychee_lib::ratelimit::HostStatsMap;
use tabled::{
    Table, Tabled,
    settings::{Alignment, Modify, Style, object::Segment},
};

pub(crate) struct MarkdownHostStats {
    pub(crate) host_stats: Option<HostStatsMap>,
}

impl Display for MarkdownHostStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(host_stats) = &self.host_stats else {
            return Ok(());
        };

        writeln!(f, "\n## Per-host Statistics")?;
        writeln!(f)?;
        writeln!(f, "{}", host_stats_table(host_stats))?;

        Ok(())
    }
}

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

fn host_stats_table(host_stats: &HostStatsMap) -> String {
    let sorted_hosts = host_stats.sorted();

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
