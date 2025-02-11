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

use std::{
    collections::{HashMap, HashSet},
    fmt::Display
};

use crate::stats::ResponseStats;
use lychee_lib::InputSource;
use anyhow::Result;

pub(crate) trait StatsFormatter {
    /// Format the stats of all responses and write them to stdout
    fn format(&self, stats: ResponseStats) -> Result<Option<String>>;
}

// Convert error_map to a sorted Vec of key-value pairs
fn sort_stat_map<T>(error_map: &HashMap<InputSource, HashSet<T>>) -> Vec<(&InputSource, &HashSet<T>)>
where T: Display
{
    let mut errors: Vec<(&InputSource, &HashSet<T>)> = error_map.iter().collect();

    errors.sort_by(|(source, _), (other_source, _)| source.cmp(other_source));

    errors
}