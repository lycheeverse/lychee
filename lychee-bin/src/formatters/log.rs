use log::Level;
use std::io::Write;

use crate::{
    formatters::{self, response::TOTAL_RESPONSE_OUTPUT_WIDTH},
    options::ResponseFormat,
    verbosity::Verbosity,
};

/// Initialize the logging system with the given verbosity level
pub(crate) fn init_logging(verbose: &Verbosity, mode: &ResponseFormat) {
    let mut builder = env_logger::Builder::new();

    builder
        .format_timestamp(None) // Disable timestamps
        .format_module_path(false) // Disable module path to reduce clutter
        .format_target(false) // Disable target
        .filter_module("lychee", verbose.log_level_filter()) // Re-add module filtering
        .filter_module("lychee_lib", verbose.log_level_filter()); // Re-add module filtering

    // Format the log messages if the output is not plain
    if !matches!(mode, ResponseFormat::Plain) {
        builder.format(|buf, record| {
            let level = record.level();

            // Spaces added to align the log levels
            let level_text = match level {
                Level::Error => "ERROR",
                Level::Warn => " WARN",
                Level::Info => " INFO",
                Level::Debug => "DEBUG",
                Level::Trace => "TRACE",
            };

            // Calculate the effective padding. Ensure it's non-negative to avoid panic.
            let padding = TOTAL_RESPONSE_OUTPUT_WIDTH.saturating_sub(level_text.len() + 2); // +2 for brackets

            // Construct the log prefix with the log level.
            let level_label = format!("[{level_text}]");
            let color = match level {
                Level::Error => &formatters::color::BOLD_PINK,
                Level::Warn => &formatters::color::BOLD_YELLOW,
                Level::Info | Level::Debug => &formatters::color::BLUE,
                Level::Trace => &formatters::color::DIM,
            };
            let colored_level = color.apply_to(level_label);
            let prefix = format!("{}{}", " ".repeat(padding), colored_level);

            // Write formatted log message with aligned level and original log message.
            writeln!(buf, "{} {}", prefix, record.args())
        });
    }

    builder.init();
}
