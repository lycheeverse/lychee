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

#[cfg(test)]
mod plain_tests {
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
        let formatter = PlainFormatter;
        let body = mock_response_body(Status::Ok(StatusCode::OK), "https://example.com");
        assert_eq!(
            formatter.format_response(&body),
            "[200] https://example.com/"
        );
    }

    #[test]
    fn test_format_response_with_error_status() {
        let formatter = PlainFormatter;
        let body = mock_response_body(
            Status::Error(ErrorKind::InvalidUrlHost),
            "https://example.com/404",
        );
        assert_eq!(
            formatter.format_response(&body),
            "[ERROR] https://example.com/404 | Failed: URL is missing a host"
        );
    }

    #[test]
    fn test_format_response_with_excluded_status() {
        let formatter = PlainFormatter;
        let body = mock_response_body(Status::Excluded, "https://example.com/not-checked");
        assert_eq!(formatter.format_response(&body), body.to_string());
        assert_eq!(
            formatter.format_response(&body),
            "[EXCLUDED] https://example.com/not-checked | Excluded"
        );
    }

    #[test]
    fn test_format_response_with_redirect_status() {
        let formatter = PlainFormatter;
        let body = mock_response_body(
            Status::Redirected(StatusCode::MOVED_PERMANENTLY),
            "https://example.com/redirect",
        );
        assert_eq!(formatter.format_response(&body), body.to_string());
        assert_eq!(
            formatter.format_response(&body),
            "[301] https://example.com/redirect | Redirect (301 Moved Permanently): Moved Permanently"
        );
    }

    #[test]
    fn test_format_response_with_unknown_status_code() {
        let formatter = PlainFormatter;
        let body = mock_response_body(
            Status::UnknownStatusCode(StatusCode::from_u16(999).unwrap()),
            "https://example.com/unknown",
        );
        assert_eq!(formatter.format_response(&body), body.to_string());
        // Check the actual string representation of the status code
        assert_eq!(
            formatter.format_response(&body),
            "[999] https://example.com/unknown | Unknown status (999 <unknown status code>)"
        );
    }
}
