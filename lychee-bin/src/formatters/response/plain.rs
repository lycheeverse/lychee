use lychee_lib::ResponseBody;

use super::ResponseBodyFormatter;

/// A basic formatter that just returns the response body as a string
/// without any color codes or other formatting.
///
/// Under the hood, it calls the `Display` implementation of the `ResponseBody`
/// type.
///
/// This formatter is used when the user has requested raw output
/// or when the terminal does not support color.
pub(crate) struct PlainFormatter;

impl ResponseBodyFormatter for PlainFormatter {
    fn format_response(&self, body: &ResponseBody) -> String {
        body.to_string()
    }
}
