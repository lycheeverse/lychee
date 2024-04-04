use env_logger::{Builder, Env};
use log::LevelFilter;
use std::io::Write;

use crate::{
    formatters::{self, response::MAX_RESPONSE_OUTPUT_WIDTH},
    options::ResponseFormat,
    verbosity::Verbosity,
};

/// Initialize the logging system with the given verbosity level.
pub(crate) fn init_logging(verbose: &Verbosity, response_format: &ResponseFormat) {
    // Set a base level for all modules to `warn`, which is a reasonable default.
    // It will be overridden by RUST_LOG if it's set.
    let base_level = "warn";
    let env = Env::default().filter_or("RUST_LOG", base_level);

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
        builder.filter_level(LevelFilter::Info); // Set to `Info` or another level as you see fit.

        // Apply more specific filters to your own crates, enabling more verbose logging as per `-vv`.
        builder
            .filter_module("lychee", level_filter)
            .filter_module("lychee_lib", level_filter);
    }

    // Customize the log message format if not plain.
    if !response_format.is_plain() {
        builder.format(move |buf, record| {
            let level = record.level();
            let level_text = format!("{level:5}");
            let max_level_text_width = 7; // Longest log level text, including brackets.
            let padding = (MAX_RESPONSE_OUTPUT_WIDTH.saturating_sub(max_level_text_width)).max(0);
            let prefix = format!(
                "{:<width$}",
                format!("[{}]", level_text),
                width = max_level_text_width
            );
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
