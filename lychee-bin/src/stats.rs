use std::collections::{HashMap, HashSet};

use lychee_lib::{Input, Response, ResponseBody, Status};
use serde::Serialize;

use crate::color::{DIM, GREEN, NORMAL, PINK, YELLOW};

pub(crate) fn color_response(response: &ResponseBody) -> String {
    let out = match response.status {
        Status::Ok(_) => GREEN.apply_to(response),
        Status::Excluded | Status::Unsupported(_) => DIM.apply_to(response),
        Status::Redirected(_) => NORMAL.apply_to(response),
        Status::UnknownStatusCode(_) | Status::Timeout(_) => YELLOW.apply_to(response),
        Status::Error(_) => PINK.apply_to(response),
    };
    out.to_string()
}

#[derive(Default, Serialize)]
pub(crate) struct ResponseStats {
    pub(crate) total: usize,
    pub(crate) successful: usize,
    pub(crate) failures: usize,
    pub(crate) unknown: usize,
    pub(crate) timeouts: usize,
    pub(crate) redirects: usize,
    pub(crate) excludes: usize,
    pub(crate) errors: usize,
    pub(crate) fail_map: HashMap<Input, HashSet<ResponseBody>>,
}

impl ResponseStats {
    #[inline]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn add(&mut self, response: Response) {
        let Response(source, ResponseBody { ref status, .. }) = response;

        // Silently skip unsupported URIs
        if status.is_unsupported() {
            return;
        }

        self.total += 1;

        match status {
            Status::Ok(_) => self.successful += 1,
            Status::Error(_) => self.failures += 1,
            Status::UnknownStatusCode(_) => self.unknown += 1,
            Status::Timeout(_) => self.timeouts += 1,
            Status::Redirected(_) => self.redirects += 1,
            Status::Excluded => self.excludes += 1,
            Status::Unsupported(_) => (), // Just skip unsupported URI
        }

        if matches!(
            status,
            Status::Error(_) | Status::Timeout(_) | Status::Redirected(_)
        ) {
            let fail = self.fail_map.entry(source).or_default();
            fail.insert(response.1);
        };
    }

    #[inline]
    pub(crate) const fn is_success(&self) -> bool {
        self.total == self.successful + self.excludes
    }

    #[inline]
    pub(crate) const fn is_empty(&self) -> bool {
        self.total == 0
    }
}

#[cfg(test)]
mod test {
    use std::collections::{HashMap, HashSet};

    use http::StatusCode;
    use lychee_lib::{ClientBuilder, Input, Response, ResponseBody, Status, Uri};
    use pretty_assertions::assert_eq;
    use reqwest::Url;
    use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};

    use super::ResponseStats;

    fn website(url: &str) -> Uri {
        Uri::from(Url::parse(url).expect("Expected valid Website URI"))
    }

    async fn get_mock_status_response<S>(status_code: S) -> Response
    where
        S: Into<StatusCode>,
    {
        let mock_server = MockServer::start().await;
        let template = ResponseTemplate::new(status_code.into());

        Mock::given(path("/"))
            .respond_with(template)
            .mount(&mock_server)
            .await;

        ClientBuilder::default()
            .client()
            .unwrap()
            .check(mock_server.uri())
            .await
            .unwrap()
    }

    #[test]
    fn test_stats_is_empty() {
        let mut stats = ResponseStats::new();
        assert!(stats.is_empty());

        stats.add(Response(
            Input::Stdin,
            ResponseBody {
                uri: website("https://example.org/ok"),
                status: Status::Ok(StatusCode::OK),
            },
        ));

        assert!(!stats.is_empty());
    }

    #[tokio::test]
    async fn test_stats() {
        let status_codes = [
            StatusCode::OK,
            StatusCode::PERMANENT_REDIRECT,
            StatusCode::BAD_GATEWAY,
        ];

        let mut stats = ResponseStats::new();
        for status in &status_codes {
            stats.add(get_mock_status_response(status).await);
        }

        let mut expected_map: HashMap<Input, HashSet<ResponseBody>> = HashMap::new();
        for status in &status_codes {
            if status.is_server_error() || status.is_client_error() || status.is_redirection() {
                let Response(input, response_body) = get_mock_status_response(status).await;
                let entry = expected_map.entry(input).or_default();
                entry.insert(response_body);
            }
        }

        assert_eq!(stats.fail_map, expected_map);
    }
}
