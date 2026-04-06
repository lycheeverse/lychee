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
    use lychee_lib::{ErrorKind, Redirect, Redirects, Status, Uri};
    use test_utils::mock_response_body;

    #[test]
    fn test_format_response_with_ok_status() {
        let formatter = TaskFormatter;
        let body = mock_response_body!(Status::Ok(StatusCode::OK), "https://example.com");
        assert_eq!(
            formatter.format_response(&body),
            "- [ ] [200] https://example.com/"
        );
    }

    #[test]
    fn test_format_response_with_error_status() {
        let formatter = TaskFormatter;
        let body = mock_response_body!(
            Status::Error(ErrorKind::EmptyUrl),
            "https://example.com/404",
        );
        assert_eq!(
            formatter.format_response(&body),
            "- [ ] [ERROR] https://example.com/404 | Empty URL found but a URL must not be empty"
        );
    }

    #[test]
    fn test_format_response_with_excluded_status() {
        let formatter = TaskFormatter;
        let body = mock_response_body!(Status::Excluded, "https://example.com/not-checked");
        assert_eq!(
            formatter.format_response(&body),
            "- [ ] [EXCLUDED] https://example.com/not-checked | This is due to your 'exclude' values"
        );
    }

    #[test]
    fn test_format_response_with_redirect_status() {
        let formatter = TaskFormatter;
        let mut redirects = Redirects::new("https://from.dev".try_into().unwrap());
        redirects.push(Redirect {
            url: "https://to.dev".try_into().unwrap(),
            code: StatusCode::PERMANENT_REDIRECT,
        });

        let body = ResponseBody {
            uri: Uri::try_from("https://example.com/redirect").unwrap(),
            status: Status::Ok(StatusCode::OK),
            redirects: Some(redirects),
            remap: None,
            span: None,
            duration: None,
        };
        assert_eq!(
            formatter.format_response(&body),
            "- [ ] [200] https://example.com/redirect | 200 OK | Followed 1 redirect. Redirects: https://from.dev/ --[308]--> https://to.dev/"
        );
    }

    #[test]
    fn test_format_response_with_unknown_status_code() {
        let formatter = TaskFormatter;
        let body = mock_response_body!(
            Status::UnknownStatusCode(StatusCode::from_u16(999).unwrap()),
            "https://example.com/unknown",
        );
        assert_eq!(
            formatter.format_response(&body),
            "- [ ] [999] https://example.com/unknown | Unknown status (999 <unknown status code>)"
        );
    }
}
