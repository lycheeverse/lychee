use anyhow::Result;

use super::StatsFormatter;
use crate::formatters::stats::OutputStats;
pub(crate) struct Raw;

impl Raw {
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl StatsFormatter for Raw {
    /// Don't print stats in raw mode
    fn format(&self, _: OutputStats) -> Result<String> {
        Ok(String::new())
    }
}
