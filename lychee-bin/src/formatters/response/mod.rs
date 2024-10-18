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
pub(crate) trait ResponseFormatter: Send + Sync {
    /// Format the response body into a human-readable string
    fn format_response(&self, body: &ResponseBody) -> String;

    /// Detailed response formatter (defaults to the normal formatter)
    ///
    /// This can be used for output modes which want to provide more detailed
    /// information. It is also used if the output is set to verbose mode
    /// (i.e. `-v`, `-vv` and above).
    fn format_detailed_response(&self, body: &ResponseBody) -> String {
        self.format_response(body)
    }
}
