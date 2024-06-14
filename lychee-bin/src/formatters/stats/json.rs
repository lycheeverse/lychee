use anyhow::{Context, Result};

use super::StatsFormatter;
use crate::stats::ResponseStats;

pub(crate) struct Json;

impl Json {
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl StatsFormatter for Json {
    /// Format stats as JSON object
    fn format(&self, stats: ResponseStats) -> Result<Option<String>> {
        serde_json::to_string_pretty(&stats)
            .map(Some)
            .context("Cannot format stats as JSON")
    }
}
