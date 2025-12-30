use anyhow::{Context, Result};

use super::StatsFormatter;
use crate::formatters::stats::OutputStats;

pub(crate) struct Json;

impl Json {
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl StatsFormatter for Json {
    /// Format stats as JSON object
    fn format(&self, stats: OutputStats) -> Result<String> {
        serde_json::to_string_pretty(&stats).context("Cannot format stats as JSON")
    }
}
