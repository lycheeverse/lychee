use lychee_lib::ResponseBody;

mod color;
mod emoji;
mod plain;

pub(crate) use color::ColorFormatter;
pub(crate) use emoji::EmojiFormatter;
pub(crate) use plain::PlainFormatter;

/// Desired total width of formatted string for color formatter
///
/// The longest string, which needs to be formatted, is currently `[Excluded]`
/// which is 10 characters long (including brackets).
///
/// Keep in sync with `Status::code_as_string`, which converts status codes to
/// strings.
pub(crate) const MAX_RESPONSE_OUTPUT_WIDTH: usize = 10;

/// A trait for formatting a response body
///
/// This trait is used to convert response body into a human-readable string.
/// It can be implemented for different formatting styles such as
/// colorized output or plaintext.
pub(crate) trait ResponseBodyFormatter: Send + Sync {
    fn format_response(&self, body: &ResponseBody) -> String;
}
