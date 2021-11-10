use super::StatsWriter;
use crate::stats::ResponseStats;
use anyhow::Result;

pub struct Markdown;

impl Markdown {
    pub(crate) fn new() -> Self {
        Markdown {}
    }
}

impl StatsWriter for Markdown {
    fn write(&self, stats: &ResponseStats) -> Result<String> {
        Ok(stats.to_string())
    }
}
