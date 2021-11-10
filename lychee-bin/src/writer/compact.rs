use crate::stats::ResponseStats;

use super::StatsWriter;

use anyhow::Result;

pub struct Compact;

impl Compact {
    pub(crate) fn new() -> Self {
        Compact {}
    }
}

impl StatsWriter for Compact {
    fn write(&self, stats: &ResponseStats) -> Result<String> {
        Ok(stats.to_string())
    }
}
