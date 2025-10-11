use env_logger::{Builder, Env};
use log::LevelFilter;
use std::io::Write;

use crate::{
    formatters::{self, response::MAX_RESPONSE_OUTPUT_WIDTH},
    options::OutputMode,
    verbosity::Verbosity,
};

/// Initialize the logging system with the given verbosity level.
pub(crate) fn init_logging(verbose: &Verbosity, mode: &OutputMode) {
    // Set a base level for all modules to `warn`, which is a reasonable default.
    // It will be overridden by RUST_LOG if it's set.
    let env = Env::default().filter_or("RUST_LOG", "warn");

    let mut builder = Builder::from_env(env);
    builder
        .format_timestamp(None)
        .format_module_path(false)
        .format_target(false);

    if std::env::var("RUST_LOG").is_err() {
        // Adjust the base log level filter based on the verbosity from CLI.
        // This applies to all modules not explicitly mentioned in RUST_LOG.
        let level_filter = verbose.log_level_filter();

        // Apply a global filter. This ensures that, by default, other modules don't log at the debug level.
        builder.filter_level(LevelFilter::Info);

        // Apply more specific filters to your own crates, enabling more verbose logging as per `-vv`.
        builder
            .filter_module("lychee", level_filter)
            .filter_module("lychee_lib", level_filter);
    }

    // Calculate the longest log level text, including brackets.
    let max_level_text_width = log::LevelFilter::iter()
        .map(|level| level.as_str().len() + 2)
        .max()
        .unwrap_or(0);

    // Customize the log message format according to the output mode
    if mode.is_plain() {
        // Explicitly disable colors for plain output
        builder.format(move |buf, record| writeln!(buf, "[{}] {}", record.level(), record.args()));
    } else if mode.is_emoji() {
        // Disable padding, keep colors
        builder.format(move |buf, record| {
            let level = record.level();
            let color = formatters::color::color_for_level(level);
            writeln!(
                buf,
                "{} {}",
                color.apply_to(format!("[{level}]")),
                record.args()
            )
        });
    } else {
        builder.format(move |buf, record| {
            let level = record.level();
            let level_text = format!("{level}");
            let padding = (MAX_RESPONSE_OUTPUT_WIDTH.saturating_sub(max_level_text_width)).max(0);
            let level_padding = max_level_text_width.saturating_sub(level_text.len() + 2);
            let prefix = format!("{:>width$}[{level_text}]", "", width = level_padding);
            let color = formatters::color::color_for_level(level);
            let colored_level = color.apply_to(&prefix);
            writeln!(
                buf,
                "{:<padding$}{} {}",
                "",
                colored_level,
                record.args(),
                padding = padding
            )
        });
    }

    builder.init();
}
