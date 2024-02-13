pub(crate) mod duration;
pub(crate) mod response;
pub(crate) mod stats;

use supports_color::Stream;

use self::response::{BasicFormatter, ColorFormatter, ResponseBodyFormatter};

/// Supported response formats
pub enum Format {
    /// Raw output (no color)
    Raw,
    /// Colorized output (if supported)
    Color,
}

/// Detects whether a terminal supports color, and gives details about that
/// support. It takes into account the `NO_COLOR` environment variable.
fn supports_color() -> bool {
    supports_color::on(Stream::Stdout).is_some()
}

/// Create a response formatter based on the given format option
pub(crate) fn get_body_formatter(format: &Format) -> Box<dyn ResponseBodyFormatter> {
    if matches!(format, Format::Raw) || !supports_color() {
        return Box::new(BasicFormatter);
    }
    Box::new(ColorFormatter)
}
