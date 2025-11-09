use lychee_lib::ResponseBody;

use super::ResponseFormatter;

/// A basic formatter that just returns the response body as a string
/// without any color codes or other formatting.
///
/// Under the hood, it calls the `Display` implementation of the `ResponseBody`
/// type.
///
/// This formatter is used when the user has requested raw output
/// or when the terminal does not support color.
pub(crate) struct PlainFormatter;

impl ResponseFormatter for PlainFormatter {
    fn format_response(&self, body: &ResponseBody) -> String {
        format!("[{}] {}", body.status.code_as_string(), body)
    }
}

#[cfg(test)]
mod plain_tests {
    use super::*;
    use http::StatusCode;
    use lychee_lib::Redirects;
    use lychee_lib::{ErrorKind, Status, Uri};
    use test_utils::mock_response_body;

    #[test]
    fn test_format_response_with_ok_status() {
        let formatter = PlainFormatter;
        let body = mock_response_body!(Status::Ok(StatusCode::OK), "https://example.com");
        assert_eq!(
            formatter.format_response(&body),
            "[200] https://example.com/"
        );
    }

    #[test]
    fn test_format_response_with_error_status() {
        let formatter = PlainFormatter;
        let body = mock_response_body!(
            Status::Error(ErrorKind::EmptyUrl),
            "https://example.com/404",
        );
        assert_eq!(
            formatter.format_response(&body),
            "[ERROR] https://example.com/404 | URL cannot be empty: Empty URL found. Check for missing links or malformed markdown"
        );
    }

    #[test]
    fn test_format_response_with_excluded_status() {
        let formatter = PlainFormatter;
        let body = mock_response_body!(Status::Excluded, "https://example.com/not-checked");
        assert_eq!(
            formatter.format_response(&body),
            "[EXCLUDED] https://example.com/not-checked"
        );
    }

    #[test]
    fn test_format_response_with_redirect_status() {
        let formatter = PlainFormatter;
        let body = mock_response_body!(
            Status::Redirected(
                StatusCode::MOVED_PERMANENTLY,
                Redirects::from(vec![
                    "https://from.dev".try_into().unwrap(),
                    "https://to.dev".try_into().unwrap(),
                ]),
            ),
            "https://example.com/redirect",
        );
        assert_eq!(
            formatter.format_response(&body),
            "[301] https://example.com/redirect | Redirect: Followed 1 redirect resolving to the final status of: Moved Permanently. Redirects: https://from.dev/ --> https://to.dev/"
        );
    }

    #[test]
    fn test_format_response_with_unknown_status_code() {
        let formatter = PlainFormatter;
        let body = mock_response_body!(
            Status::UnknownStatusCode(StatusCode::from_u16(999).unwrap()),
            "https://example.com/unknown",
        );
        assert_eq!(
            formatter.format_response(&body),
            "[999] https://example.com/unknown | Unknown status (999 <unknown status code>)"
        );
    }
}
