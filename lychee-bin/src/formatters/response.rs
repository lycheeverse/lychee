use lychee_lib::{CacheStatus, ResponseBody, Status};

use super::color::{DIM, GREEN, NORMAL, PINK, YELLOW};

/// A trait for formatting a response body
///
/// This trait is used to format a response body into a string.
/// It can be implemented for different formatting styles such as
/// colorized output or plain text.
pub(crate) trait ResponseBodyFormatter: Send + Sync {
    fn format_response(&self, body: &ResponseBody) -> String;
}

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

/// A colorized formatter for the response body
///
/// This formatter is used when the terminal supports color and the user
/// has not explicitly requested raw, uncolored output.
pub(crate) struct ColorFormatter;

impl ResponseBodyFormatter for ColorFormatter {
    fn format_response(&self, body: &ResponseBody) -> String {
        // Determine the color based on the status.
        let status_color = match body.status {
            Status::Ok(_) | Status::Cached(CacheStatus::Ok(_)) => &GREEN,
            Status::Excluded
            | Status::Unsupported(_)
            | Status::Cached(CacheStatus::Excluded | CacheStatus::Unsupported) => &DIM,
            Status::Redirected(_) => &NORMAL,
            Status::UnknownStatusCode(_) | Status::Timeout(_) => &YELLOW,
            Status::Error(_) | Status::Cached(CacheStatus::Error(_)) => &PINK,
        };

        let status_formatted = format_status(&body.status);

        let colored_status = status_color.apply_to(status_formatted);

        // Construct the output.
        format!("{} {}", colored_status, body.uri)
    }
}

/// Desired total width of formatted string for color formatter
///
/// The longest string, which needs to be formatted, is currently `[Excluded]`
/// which is 10 characters long (including brackets).
///
/// Keep in sync with `Status::code_as_string`, which converts status codes to
/// strings.
pub(crate) const TOTAL_RESPONSE_OUTPUT_WIDTH: usize = 10;

/// Format the status code or text for the color formatter.
///
/// Numeric status codes are right-aligned.
/// Textual statuses are left-aligned.
/// Padding is taken into account.
fn format_status(status: &Status) -> String {
    let status_code_or_text = status.code_as_string();

    // Calculate the effective padding. Ensure it's non-negative to avoid panic.
    let padding = TOTAL_RESPONSE_OUTPUT_WIDTH.saturating_sub(status_code_or_text.len() + 2); // +2 for brackets

    format!(
        "{}[{:>width$}]",
        " ".repeat(padding),
        status_code_or_text,
        width = status_code_or_text.len()
    )
}

/// An emoji formatter for the response body
///
/// This formatter replaces certain textual elements with emojis for a more
/// visual output.
pub(crate) struct EmojiFormatter;

impl ResponseBodyFormatter for EmojiFormatter {
    fn format_response(&self, body: &ResponseBody) -> String {
        let emoji = match body.status {
            Status::Ok(_) | Status::Cached(CacheStatus::Ok(_)) => "✅",
            Status::Excluded
            | Status::Unsupported(_)
            | Status::Cached(CacheStatus::Excluded | CacheStatus::Unsupported) => "🚫",
            Status::Redirected(_) => "↪️",
            Status::UnknownStatusCode(_) | Status::Timeout(_) => "⚠️",
            Status::Error(_) | Status::Cached(CacheStatus::Error(_)) => "❌",
        };
        format!("{} {}", emoji, body.uri)
    }
}
