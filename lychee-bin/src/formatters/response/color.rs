use lychee_lib::{CacheStatus, ResponseBody, Status};

use crate::formatters::color::{DIM, GREEN, NORMAL, PINK, YELLOW};

use super::{ResponseFormatter, MAX_RESPONSE_OUTPUT_WIDTH};

/// A colorized formatter for the response body
///
/// This formatter is used if the terminal supports color and the user
/// has not explicitly requested raw, uncolored output.
pub(crate) struct ColorFormatter;

impl ColorFormatter {
    /// Determine the color for formatted output based on the status of the
    /// response.
    fn status_color(status: &Status) -> &'static once_cell::sync::Lazy<console::Style> {
        match status {
            Status::Ok(_) | Status::Cached(CacheStatus::Ok(_)) => &GREEN,
            Status::Excluded
            | Status::Unsupported(_)
            | Status::Cached(CacheStatus::Excluded | CacheStatus::Unsupported) => &DIM,
            Status::Redirected(_) => &NORMAL,
            Status::UnknownStatusCode(_) | Status::Timeout(_) => &YELLOW,
            Status::Error(_) | Status::Cached(CacheStatus::Error(_)) => &PINK,
        }
    }

    /// Format the status code or text for the color formatter.
    ///
    /// - Numeric status codes are right-aligned.
    /// - Textual statuses are left-aligned.
    /// - Padding is taken into account.
    fn format_status(status: &Status) -> String {
        let status_code_or_text = status.code_as_string();

        // Calculate the effective padding. Ensure it's non-negative to avoid panic.
        let padding = MAX_RESPONSE_OUTPUT_WIDTH.saturating_sub(status_code_or_text.len() + 2); // +2 for brackets

        format!(
            "{}[{:>width$}]",
            " ".repeat(padding),
            status_code_or_text,
            width = status_code_or_text.len()
        )
    }

    /// Color and format the response status.
    fn format_response_status(status: &Status) -> String {
        let status_color = ColorFormatter::status_color(status);
        let formatted_status = ColorFormatter::format_status(status);
        status_color.apply_to(formatted_status).to_string()
    }
}

impl ResponseFormatter for ColorFormatter {
    fn format_response(&self, body: &ResponseBody) -> String {
        let colored_status = ColorFormatter::format_response_status(&body.status);
        format!("{} {}", colored_status, body.uri)
    }

    /// Provide some more detailed information about the response
    /// This prints the entire response body, including the exact error message
    /// (if available).
    fn format_detailed_response(&self, body: &ResponseBody) -> String {
        let colored_status = ColorFormatter::format_response_status(&body.status);
        format!("{colored_status} {body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;
    use lychee_lib::{ErrorKind, Status, Uri};

    // Helper function to create a ResponseBody with a given status and URI
    fn mock_response_body(status: Status, uri: &str) -> ResponseBody {
        ResponseBody {
            uri: Uri::try_from(uri).unwrap(),
            status,
        }
    }

    #[cfg(test)]
    /// Helper function to strip ANSI color codes for tests
    fn strip_ansi_codes(s: &str) -> String {
        console::strip_ansi_codes(s).to_string()
    }

    #[test]
    fn test_format_status() {
        let status = Status::Ok(StatusCode::OK);
        assert_eq!(ColorFormatter::format_status(&status).trim_start(), "[200]");
    }

    #[test]
    fn test_format_response_with_ok_status() {
        let formatter = ColorFormatter;
        let body = mock_response_body(Status::Ok(StatusCode::OK), "https://example.com");
        assert_eq!(
            strip_ansi_codes(&formatter.format_response(&body)),
            "     [200] https://example.com/"
        );
    }

    #[test]
    fn test_format_response_with_error_status() {
        let formatter = ColorFormatter;
        let body = mock_response_body(
            Status::Error(ErrorKind::InvalidUrlHost),
            "https://example.com/404",
        );
        assert_eq!(
            strip_ansi_codes(&formatter.format_response(&body)),
            "   [ERROR] https://example.com/404"
        );
    }

    #[test]
    fn test_format_response_with_long_uri() {
        let formatter = ColorFormatter;
        let long_uri =
            "https://example.com/some/very/long/path/to/a/resource/that/exceeds/normal/lengths";
        let body = mock_response_body(Status::Ok(StatusCode::OK), long_uri);
        let formatted_response = formatter.format_response(&body);
        assert!(formatted_response.contains(long_uri));
    }

    #[test]
    fn test_detailed_response_output() {
        let formatter = ColorFormatter;
        let body = mock_response_body(
            Status::Error(ErrorKind::InvalidUrlHost),
            "https://example.com/404",
        );

        let response = formatter.format_detailed_response(&body);

        assert_eq!(
            response,
            "\u{1b}[38;5;197m   [ERROR]\u{1b}[0m [ERROR] https://example.com/404 | URL is missing a host"
        );
    }
}
