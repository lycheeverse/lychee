use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Display},
};

use crate::color::*;
use lychee_lib::{Input, Response, ResponseBody, Status};
use serde::Serialize;

pub(crate) fn color_response(response: &ResponseBody) -> String {
    let out = match response.status {
        Status::Ok(_) => GREEN.apply_to(response),
        Status::Excluded | Status::Unsupported(_) => DIM.apply_to(response),
        Status::Redirected(_) => NORMAL.apply_to(response),
        Status::UnknownStatusCode(_) | Status::Timeout(_) => YELLOW.apply_to(response),
        Status::Error(_) => MAGENTA.apply_to(response),
    };
    out.to_string()
}

#[derive(Default, Serialize)]
pub(crate) struct ResponseStats {
    total: usize,
    successful: usize,
    failures: usize,
    unknown: usize,
    timeouts: usize,
    redirects: usize,
    excludes: usize,
    errors: usize,
    fail_map: HashMap<Input, HashSet<ResponseBody>>,
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

    // Helper function, which prints the detailed list of errors
    fn print_errors(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut errors = HashMap::new();
        errors.insert("HTTP", self.failures);
        errors.insert("Redirects", self.redirects);
        errors.insert("Timeouts", self.timeouts);
        errors.insert("Unknown", self.unknown);

        // Creates an output like `(HTTP:3|Timeouts:1|Unknown:1)`
        let mut error_str: Vec<_> = errors
            .into_iter()
            .filter(|(_, v)| *v > 0)
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect();
        error_str.sort();

        color!(f, MAGENTA, "({})", error_str.join("|"))?;
        Ok(())
    }
}

impl Display for ResponseStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        color!(
            f,
            BOLD_MAGENTA,
            "Issues found in {} inputs. Find details below.\n\n",
            self.fail_map.len()
        )?;
        for (input, responses) in &self.fail_map {
            color!(f, BOLD_YELLOW, "[{}]:\n", input)?;
            for response in responses {
                writeln!(f, "{}", color_response(response))?;
            }
            writeln!(f)?;
        }

        color!(f, NORMAL, "\u{1F50D} {} Total", self.total)?;
        color!(f, BOLD_GREEN, " \u{2705} {} OK", self.successful)?;
        color!(
            f,
            BOLD_MAGENTA,
            " \u{1f6ab} {} Errors",
            self.errors + self.failures
        )?;
        if self.errors + self.failures > 0 {
            write!(f, " ")?;
            self.print_errors(f)?;
        }
        if self.excludes > 0 {
            color!(f, BOLD_YELLOW, " \u{1F4A4} Excluded: {}", self.excludes)?;
        }
        Ok(())
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
        let stata = [
            StatusCode::OK,
            StatusCode::PERMANENT_REDIRECT,
            StatusCode::BAD_GATEWAY,
        ];

        let mut stats = ResponseStats::new();
        for status in &stata {
            stats.add(get_mock_status_response(status).await);
        }

        let mut expected_map: HashMap<Input, HashSet<ResponseBody>> = HashMap::new();
        for status in &stata {
            if status.is_server_error() || status.is_client_error() || status.is_redirection() {
                let Response(input, response_body) = get_mock_status_response(status).await;
                let entry = expected_map.entry(input).or_default();
                entry.insert(response_body);
            }
        }

        assert_eq!(stats.fail_map, expected_map);
    }
}
