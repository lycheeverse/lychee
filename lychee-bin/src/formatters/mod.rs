pub(crate) mod color;
pub(crate) mod duration;
pub(crate) mod log;
pub(crate) mod response;
pub(crate) mod stats;

use self::{response::ResponseFormatter, stats::StatsFormatter};
use crate::options::{OutputMode, StatsFormat};
use supports_color::Stream;

/// Detects whether a terminal supports color, and gives details about that
/// support. It takes into account the `NO_COLOR` environment variable.
fn supports_color() -> bool {
    supports_color::on(Stream::Stdout).is_some()
}

/// Create a stats formatter based on the given format option
pub(crate) fn get_stats_formatter(
    format: &StatsFormat,
    mode: &OutputMode,
) -> Box<dyn StatsFormatter> {
    match format {
        StatsFormat::Compact => Box::new(stats::Compact::new(mode.clone())),
        StatsFormat::Detailed => Box::new(stats::Detailed::new(mode.clone())),
        StatsFormat::Json => Box::new(stats::Json::new()),
        StatsFormat::Markdown => Box::new(stats::Markdown::new()),
        StatsFormat::Raw => Box::new(stats::Raw::new()),
    }
}

/// Create a response formatter based on the given format option
pub(crate) fn get_response_formatter(mode: &OutputMode) -> Box<dyn ResponseFormatter> {
    if !supports_color() {
        return Box::new(response::PlainFormatter);
    }
    match mode {
        OutputMode::Plain => Box::new(response::PlainFormatter),
        OutputMode::Color => Box::new(response::ColorFormatter),
        OutputMode::Emoji => Box::new(response::EmojiFormatter),
        OutputMode::Task => Box::new(response::TaskFormatter),
    }
}
