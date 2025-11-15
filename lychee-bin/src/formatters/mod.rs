pub(crate) mod color;
pub(crate) mod duration;
pub(crate) mod log;
pub(crate) mod response;
pub(crate) mod stats;
pub(crate) mod suggestion;

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

/// Create a progress formatter based on the given format option
pub(crate) fn get_progress_formatter(mode: &OutputMode) -> Box<dyn ResponseFormatter> {
    let mode = match mode {
        OutputMode::Plain => OutputMode::Plain,
        OutputMode::Color => OutputMode::Color,
        OutputMode::Emoji => OutputMode::Emoji,
        OutputMode::Task => OutputMode::default(),
    };

    get_response_formatter(&mode)
}

/// Create a response formatter based on the given format option
pub(crate) fn get_response_formatter(mode: &OutputMode) -> Box<dyn ResponseFormatter> {
    // Checks if color is supported in current environment or NO_COLOR is set (https://no-color.org)
    if !supports_color() {
        // To fix `TaskFormatter` not working if color is not supported
        return match mode {
            OutputMode::Task => Box::new(response::TaskFormatter),
            _ => Box::new(response::PlainFormatter),
        };
    }
    match mode {
        OutputMode::Plain => Box::new(response::PlainFormatter),
        OutputMode::Color => Box::new(response::ColorFormatter),
        OutputMode::Emoji => Box::new(response::EmojiFormatter),
        OutputMode::Task => Box::new(response::TaskFormatter),
    }
}
