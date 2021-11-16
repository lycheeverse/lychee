mod compact;
mod detailed;
mod json;
mod markdown;

use crate::stats::ResponseStats;
use anyhow::Result;

pub(crate) use compact::Compact;
pub(crate) use detailed::Detailed;
pub(crate) use json::Json;
pub(crate) use markdown::Markdown;

pub(crate) trait StatsWriter {
    fn write(&self, stats: ResponseStats) -> Result<String>;
}
