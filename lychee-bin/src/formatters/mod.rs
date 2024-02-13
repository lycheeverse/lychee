pub(crate) mod duration;
pub(crate) mod response;
pub(crate) mod stats;

use supports_color::Stream;

use crate::options::Format;

use self::response::{BasicFormatter, ColorFormatter, ResponseBodyFormatter};

/// Detects whether a terminal supports color, and gives details about that
/// support. It takes into account the `NO_COLOR` environment variable.
fn supports_color() -> bool {
    supports_color::on(Stream::Stdout).is_some()
}

/// Create a response formatter, which formats the response body based on the
/// format option
pub(crate) fn get_formatter(format: &Format) -> Box<dyn ResponseBodyFormatter> {
    if matches!(format, Format::Raw) || !supports_color() {
        return Box::new(BasicFormatter);
    }
    Box::new(ColorFormatter)
}
