use super::ResponseFormatter;
use lychee_lib::ResponseBody;

pub(crate) struct TaskFormatter;

impl ResponseFormatter for TaskFormatter {
    fn format_response(&self, body: &ResponseBody) -> String {
        format!("- [ ] [{}] {}", body.status.code_as_string(), body)
    }
}

#[cfg(test)]
mod task_tests {
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
        let formatter = TaskFormatter;
        let body = mock_response_body(Status::Ok(StatusCode::OK), "https://example.com");
        assert_eq!(
            formatter.format_response(&body),
            "- [ ] [200] https://example.com/"
        );
    }

    #[test]
    fn test_format_response_with_error_status() {
        let formatter = TaskFormatter;
        let body = mock_response_body(
            Status::Error(ErrorKind::InvalidUrlHost),
            "https://example.com/404",
        );
        assert_eq!(
            formatter.format_response(&body),
            "- [ ] [ERROR] https://example.com/404 | URL is missing a host"
        );
    }

    #[test]
    fn test_format_response_with_excluded_status() {
        let formatter = TaskFormatter;
        let body = mock_response_body(Status::Excluded, "https://example.com/not-checked");
        assert_eq!(
            formatter.format_response(&body),
            "- [ ] [EXCLUDED] https://example.com/not-checked"
        );
    }

    #[test]
    fn test_format_response_with_redirect_status() {
        let formatter = TaskFormatter;
        let body = mock_response_body(
            Status::Redirected(StatusCode::MOVED_PERMANENTLY),
            "https://example.com/redirect",
        );
        assert_eq!(
            formatter.format_response(&body),
            "- [ ] [301] https://example.com/redirect | Redirect (301 Moved Permanently): Moved Permanently"
        );
    }

    #[test]
    fn test_format_response_with_unknown_status_code() {
        let formatter = TaskFormatter;
        let body = mock_response_body(
            Status::UnknownStatusCode(StatusCode::from_u16(999).unwrap()),
            "https://example.com/unknown",
        );
        assert_eq!(
            formatter.format_response(&body),
            "- [ ] [999] https://example.com/unknown | Unknown status (999 <unknown status code>)"
        );
    }
}
