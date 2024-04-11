use lychee_lib::{CacheStatus, ResponseBody, Status};

use crate::formatters::color::{DIM, GREEN, NORMAL, PINK, YELLOW};

use super::{ResponseBodyFormatter, MAX_RESPONSE_OUTPUT_WIDTH};

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

/// Format the status code or text for the color formatter.
///
/// Numeric status codes are right-aligned.
/// Textual statuses are left-aligned.
/// Padding is taken into account.
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

    #[test]
    fn test_format_response_with_ok_status() {
        let formatter = ColorFormatter;
        let body = mock_response_body(Status::Ok(StatusCode::OK), "https://example.com");
        assert_eq!(
            formatter.format_response(&body),
            "\u{1b}[38;5;2m\u{1b}[1m     [200]\u{1b}[0m https://example.com/"
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
            formatter.format_response(&body),
            "\u{1b}[38;5;197m   [ERROR]\u{1b}[0m https://example.com/404"
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
}
