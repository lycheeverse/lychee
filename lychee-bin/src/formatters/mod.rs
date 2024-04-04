pub(crate) mod color;
pub(crate) mod duration;
pub(crate) mod log;
pub(crate) mod response;
pub(crate) mod stats;

use self::{response::ResponseBodyFormatter, stats::StatsFormatter};
use crate::options::{ResponseFormat, StatsFormat};
use supports_color::Stream;

/// Detects whether a terminal supports color, and gives details about that
/// support. It takes into account the `NO_COLOR` environment variable.
fn supports_color() -> bool {
    supports_color::on(Stream::Stdout).is_some()
}

pub(crate) fn get_stats_formatter(
    format: &StatsFormat,
    response_format: &ResponseFormat,
) -> Box<dyn StatsFormatter> {
    match format {
        StatsFormat::Compact => Box::new(stats::Compact::new(response_format.clone())),
        StatsFormat::Detailed => Box::new(stats::Detailed::new(response_format.clone())),
        StatsFormat::Json => Box::new(stats::Json::new()),
        StatsFormat::Markdown => Box::new(stats::Markdown::new()),
        StatsFormat::Raw => Box::new(stats::Raw::new()),
    }
}

/// Create a response formatter based on the given format option
///
pub(crate) fn get_response_formatter(format: &ResponseFormat) -> Box<dyn ResponseBodyFormatter> {
    if !supports_color() {
        return Box::new(response::PlainFormatter);
    }
    match format {
        ResponseFormat::Plain => Box::new(response::PlainFormatter),
        ResponseFormat::Color => Box::new(response::ColorFormatter),
        ResponseFormat::Emoji => Box::new(response::EmojiFormatter),
    }
}
