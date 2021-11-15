mod compact;
mod detailed;
mod json;
mod markdown;

use crate::stats::ResponseStats;
use anyhow::Result;

pub use compact::Compact;
pub use detailed::Detailed;
pub use json::Json;
pub use markdown::Markdown;

pub trait StatsWriter {
    fn write(&self, stats: ResponseStats) -> Result<String>;
}
