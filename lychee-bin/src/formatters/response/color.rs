use lychee_lib::{CacheStatus, ResponseBody, Status};

use crate::formatters::color::{DIM, GREEN, PINK, YELLOW};

use super::{MAX_RESPONSE_OUTPUT_WIDTH, ResponseFormatter};

/// A colorized formatter for the response body
///
/// This formatter is used if the terminal supports color and the user
/// has not explicitly requested raw, uncolored output.
pub(crate) struct ColorFormatter;

impl ColorFormatter {
    /// Determine the color for formatted output based on the status of the
    /// response.
    fn status_color(status: &Status) -> &'static std::sync::LazyLock<console::Style> {
        match status {
            Status::Ok(_) | Status::Cached(CacheStatus::Ok(_)) | Status::Redirected(_, _) => &GREEN,
            Status::Excluded
            | Status::Unsupported(_)
            | Status::Cached(CacheStatus::Excluded | CacheStatus::Unsupported) => &DIM,
            Status::UnknownStatusCode(_) | Status::Timeout(_) => &YELLOW,
            Status::Error(_) | Status::RequestError(_) | Status::Cached(CacheStatus::Error(_)) => {
                &PINK
            }
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
        format!("{colored_status} {body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;
    use lychee_lib::{ErrorKind, Status, Uri};
    use pretty_assertions::assert_eq;
    use test_utils::mock_response_body;

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
        let body = mock_response_body!(Status::Ok(StatusCode::OK), "https://example.com");
        let formatted_response = strip_ansi_codes(&formatter.format_response(&body));
        assert_eq!(formatted_response, "     [200] https://example.com/");
    }

    #[test]
    fn test_format_response_with_error_status() {
        let formatter = ColorFormatter;
        let body = mock_response_body!(
            Status::Error(ErrorKind::EmptyUrl),
            "https://example.com/404",
        );
        let formatted_response = strip_ansi_codes(&formatter.format_response(&body));
        assert_eq!(
            formatted_response,
            "   [ERROR] https://example.com/404 | URL cannot be empty: Empty URL found. Check for missing links or malformed markdown"
        );
    }

    #[test]
    fn test_format_response_with_long_uri() {
        let formatter = ColorFormatter;
        let long_uri =
            "https://example.com/some/very/long/path/to/a/resource/that/exceeds/normal/lengths";
        let body = mock_response_body!(Status::Ok(StatusCode::OK), long_uri);
        let formatted_response = strip_ansi_codes(&formatter.format_response(&body));
        assert!(formatted_response.contains(long_uri));
    }

    #[test]
    fn test_error_response_output() {
        let formatter = ColorFormatter;
        let body = mock_response_body!(
            Status::Error(ErrorKind::EmptyUrl),
            "https://example.com/404",
        );

        let response = strip_ansi_codes(&formatter.format_response(&body));
        assert_eq!(
            response,
            "   [ERROR] https://example.com/404 | URL cannot be empty: Empty URL found. Check for missing links or malformed markdown"
        );
    }
}
