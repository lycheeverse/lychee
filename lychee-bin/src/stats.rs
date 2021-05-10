use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Display},
};

use console::style;
use lychee_lib::{Input, Response, ResponseBody, Status};
use pad::{Alignment, PadStr};
use serde::Serialize;

// Maximum padding for each entry in the final statistics output
const MAX_PADDING: usize = 20;

pub(crate) fn color_response(response: &ResponseBody) -> String {
    let out = match response.status {
        Status::Ok(_) => style(response).green().bright(),
        Status::Excluded | Status::Unsupported(_) => style(response).dim(),
        Status::Redirected(_) => style(response),
        Status::UnknownStatusCode(_) | Status::Timeout(_) => style(response).yellow().bright(),
        Status::Error(_) => style(response).red().bright(),
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
        if status.is_unsupported() {
            // Silently skip unsupported URIs
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

fn write_stat(f: &mut fmt::Formatter, title: &str, stat: usize, newline: bool) -> fmt::Result {
    let fill = title.chars().count();
    f.write_str(title)?;
    f.write_str(
        &stat
            .to_string()
            .pad(MAX_PADDING - fill, '.', Alignment::Right, false),
    )?;

    if newline {
        f.write_str("\n")?;
    }

    Ok(())
}

impl Display for ResponseStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let separator = "-".repeat(MAX_PADDING + 1);

        writeln!(f, "\u{1f4dd} Summary")?; // 📝
        writeln!(f, "{}", separator)?;
        write_stat(f, "\u{1f50d} Total", self.total, true)?; // 🔍
        write_stat(f, "\u{2705} Successful", self.successful, true)?; // ✅
        write_stat(f, "\u{23f3} Timeouts", self.timeouts, true)?; // ⏳
        write_stat(f, "\u{1f500} Redirected", self.redirects, true)?; // 🔀
        write_stat(f, "\u{1f47b} Excluded", self.excludes, true)?; // 👻
        write_stat(f, "\u{26a0} Unknown", self.unknown, true)?; // ⚠️
        write_stat(f, "\u{1f6ab} Errors", self.errors + self.failures, false)?; // 🚫

        for (input, responses) in &self.fail_map {
            // Using leading newlines over trailing ones (e.g. `writeln!`)
            // lets us avoid extra newlines without any additional logic.
            write!(f, "\n\nErrors in {}", input)?;
            for response in responses {
                write!(f, "\n{}", color_response(response))?
            }
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
                uri: website("http://example.org/ok"),
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
