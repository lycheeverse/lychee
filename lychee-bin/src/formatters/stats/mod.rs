mod compact;
mod detailed;
mod json;
mod markdown;
mod raw;

pub(crate) use compact::Compact;
pub(crate) use detailed::Detailed;
pub(crate) use json::Json;
pub(crate) use markdown::Markdown;
pub(crate) use raw::Raw;

use crate::stats::ResponseStats;
use anyhow::Result;

pub(crate) trait StatsFormatter {
    /// Format the stats of all responses and write them to stdout
    fn format(&self, stats: ResponseStats) -> Result<Option<String>>;
}
