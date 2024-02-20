use anyhow::Result;

use super::StatsFormatter;
use crate::stats::ResponseStats;
pub(crate) struct Raw;

impl Raw {
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl StatsFormatter for Raw {
    /// Don't print stats in raw mode
    fn format(&self, _stats: ResponseStats) -> Result<Option<String>> {
        Ok(None)
    }
}
