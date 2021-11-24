use anyhow::{Context, Result};

use super::StatsWriter;
use crate::stats::ResponseStats;

pub(crate) struct Json;

impl Json {
    pub(crate) const fn new() -> Self {
        Json {}
    }
}

impl StatsWriter for Json {
    fn write(&self, stats: ResponseStats) -> Result<String> {
        serde_json::to_string_pretty(&stats).context("Cannot format stats as JSON")
    }
}
