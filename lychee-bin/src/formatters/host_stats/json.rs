use anyhow::{Context, Result};
use serde_json::json;
use std::collections::HashMap;

use super::HostStatsFormatter;
use lychee_lib::ratelimit::HostStats;

pub(crate) struct Json;

impl Json {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl HostStatsFormatter for Json {
    /// Format host stats as JSON object
    fn format(&self, host_stats: HashMap<String, HostStats>) -> Result<String> {
        // Convert HostStats to a more JSON-friendly format
        let json_stats: HashMap<String, serde_json::Value> = host_stats
            .into_iter()
            .map(|(hostname, stats)| {
                let json_value = json!({
                    "total_requests": stats.total_requests,
                    "successful_requests": stats.successful_requests,
                    "success_rate": stats.success_rate(),
                    "rate_limited": stats.rate_limited,
                    "client_errors": stats.client_errors,
                    "server_errors": stats.server_errors,
                    "median_request_time_ms": stats.median_request_time()
                        .map(|d| {
                            #[allow(clippy::cast_possible_truncation)]
                            let millis = d.as_millis() as u64;
                            millis
                        }),
                    "cache_hits": stats.cache_hits,
                    "cache_misses": stats.cache_misses,
                    "cache_hit_rate": stats.cache_hit_rate(),
                    "status_codes": stats.status_codes
                });
                (hostname, json_value)
            })
            .collect();

        let output = json!({
            "host_statistics": json_stats
        });

        serde_json::to_string_pretty(&output).context("Cannot format host stats as JSON")
    }
}
