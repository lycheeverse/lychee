use lychee_lib::{CacheStatus, ResponseBody, Status};

use super::ResponseFormatter;

/// An emoji formatter for the response body
///
/// This formatter replaces certain textual elements with emojis for a more
/// visual output.
pub(crate) struct EmojiFormatter;

impl EmojiFormatter {
    /// Determine the color for formatted output based on the status of the
    /// response.
    const fn emoji_for_status(status: &Status) -> &'static str {
        match status {
            Status::Ok(_) | Status::Cached(CacheStatus::Ok(_)) => "✅",
            Status::Excluded
            | Status::Unsupported(_)
            | Status::Cached(CacheStatus::Excluded | CacheStatus::Unsupported) => "🚫",
            Status::Redirected(_, _) => "↪️",
            Status::UnknownStatusCode(_) | Status::Timeout(_) => "⚠️",
            Status::Error(_) | Status::Cached(CacheStatus::Error(_)) => "❌",
        }
    }
}

impl ResponseFormatter for EmojiFormatter {
    fn format_response(&self, body: &ResponseBody) -> String {
        let emoji = EmojiFormatter::emoji_for_status(&body.status);
        format!("{} {}", emoji, body.uri)
    }

    fn format_detailed_response(&self, body: &ResponseBody) -> String {
        let emoji = EmojiFormatter::emoji_for_status(&body.status);
        format!("{emoji} {body}")
    }
}

#[cfg(test)]
mod emoji_tests {
    use super::*;
    use http::StatusCode;
    use lychee_lib::{ErrorKind, Redirects, Status, Uri};
    use test_utils::mock_response_body;

    #[test]
    fn test_format_response_with_ok_status() {
        let formatter = EmojiFormatter;
        let body = mock_response_body!(Status::Ok(StatusCode::OK), "https://example.com");
        assert_eq!(formatter.format_response(&body), "✅ https://example.com/");
    }

    #[test]
    fn test_format_response_with_error_status() {
        let formatter = EmojiFormatter;
        let body = mock_response_body!(
            Status::Error(ErrorKind::EmptyUrl),
            "https://example.com/404",
        );
        assert_eq!(
            formatter.format_response(&body),
            "❌ https://example.com/404"
        );
    }

    #[test]
    fn test_format_response_with_excluded_status() {
        let formatter = EmojiFormatter;
        let body = mock_response_body!(Status::Excluded, "https://example.com/not-checked");
        assert_eq!(
            formatter.format_response(&body),
            "🚫 https://example.com/not-checked"
        );
    }

    #[test]
    fn test_format_response_with_redirect_status() {
        let formatter = EmojiFormatter;
        let body = mock_response_body!(
            Status::Redirected(StatusCode::MOVED_PERMANENTLY, Redirects::none()),
            "https://example.com/redirect",
        );
        assert_eq!(
            formatter.format_response(&body),
            "↪️ https://example.com/redirect"
        );
    }

    #[test]
    fn test_format_response_with_unknown_status_code() {
        let formatter = EmojiFormatter;
        let body = mock_response_body!(
            Status::UnknownStatusCode(StatusCode::from_u16(999).unwrap()),
            "https://example.com/unknown",
        );
        assert_eq!(
            formatter.format_response(&body),
            "⚠️ https://example.com/unknown"
        );
    }

    #[test]
    fn test_detailed_response_output() {
        let formatter = EmojiFormatter;
        let body = mock_response_body!(
            Status::Error(ErrorKind::EmptyUrl),
            "https://example.com/404",
        );

        // Just assert the output contains the expected error message
        assert!(
            formatter
                .format_detailed_response(&body)
                .contains("Empty URL found")
        );
    }
}
