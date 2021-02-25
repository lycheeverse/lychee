use console::style;
use pad::{Alignment, PadStr};
use serde::Serialize;

use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Display},
};

use lychee::{self, collector::Input, Response, Status};

// Maximum padding for each entry in the final statistics output
const MAX_PADDING: usize = 20;

pub fn color_response(response: &Response) -> String {
    let out = match response.status {
        Status::Ok(_) => style(response).green().bright(),
        Status::Redirected(_) => style(response),
        Status::Excluded => style(response).dim(),
        Status::Error(_) => style(response).yellow().bright(),
        Status::Timeout(_) => style(response).yellow().bright(),
        Status::Failed(_) => style(response).red().bright(),
    };
    out.to_string()
}

#[derive(Serialize)]
pub struct ResponseStats {
    total: usize,
    successful: usize,
    failures: usize,
    timeouts: usize,
    redirects: usize,
    excludes: usize,
    errors: usize,
    fail_map: HashMap<Input, HashSet<Response>>,
}

impl ResponseStats {
    pub fn new() -> Self {
        let fail_map = HashMap::new();
        ResponseStats {
            total: 0,
            successful: 0,
            failures: 0,
            timeouts: 0,
            redirects: 0,
            excludes: 0,
            errors: 0,
            fail_map,
        }
    }

    pub fn add(&mut self, response: Response) {
        self.total += 1;
        match response.status {
            Status::Failed(_) => self.failures += 1,
            Status::Timeout(_) => self.timeouts += 1,
            Status::Redirected(_) => self.redirects += 1,
            Status::Excluded => self.excludes += 1,
            Status::Error(_) => self.errors += 1,
            _ => self.successful += 1,
        }

        if matches!(
            response.status,
            Status::Failed(_) | Status::Timeout(_) | Status::Redirected(_) | Status::Error(_)
        ) {
            let fail = self.fail_map.entry(response.source.clone()).or_default();
            fail.insert(response);
        };
    }

    pub fn is_success(&self) -> bool {
        self.total == self.successful + self.excludes
    }
}

fn write_stat(f: &mut fmt::Formatter, title: &str, stat: usize) -> fmt::Result {
    let fill = title.chars().count();
    f.write_str(title)?;
    f.write_str(
        &stat
            .to_string()
            .pad(MAX_PADDING - fill, '.', Alignment::Right, false),
    )?;
    f.write_str("\n")
}

impl Display for ResponseStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let separator = "-".repeat(MAX_PADDING + 1);

        writeln!(f, "📝 Summary")?;
        writeln!(f, "{}", separator)?;
        write_stat(f, "🔍 Total", self.total)?;
        write_stat(f, "✅ Successful", self.successful)?;
        write_stat(f, "⏳ Timeouts", self.timeouts)?;
        write_stat(f, "🔀 Redirected", self.redirects)?;
        write_stat(f, "👻 Excluded", self.excludes)?;
        write_stat(f, "🚫 Errors", self.errors + self.failures)?;

        if !&self.fail_map.is_empty() {
            writeln!(f)?;
        }
        for (input, responses) in &self.fail_map {
            writeln!(f, "Errors in {}", input)?;
            for response in responses {
                writeln!(f, "{}", color_response(response))?
            }
            writeln!(f)?;
        }
        writeln!(f)
    }
}

#[cfg(test)]
mod test_super {
    use lychee::{test_utils::website, Status};

    use super::*;

    #[test]
    fn test_stats() {
        let mut stats = ResponseStats::new();
        stats.add(Response {
            uri: website("http://example.org/ok"),
            status: Status::Ok(http::StatusCode::OK),
            source: Input::Stdin,
        });
        stats.add(Response {
            uri: website("http://example.org/failed"),
            status: Status::Failed(http::StatusCode::BAD_GATEWAY),
            source: Input::Stdin,
        });
        stats.add(Response {
            uri: website("http://example.org/redirect"),
            status: Status::Redirected(http::StatusCode::PERMANENT_REDIRECT),
            source: Input::Stdin,
        });
        let mut expected_map = HashMap::new();
        expected_map.insert(
            Input::Stdin,
            vec![
                Response {
                    uri: website("http://example.org/failed"),
                    status: Status::Failed(http::StatusCode::BAD_GATEWAY),
                    source: Input::Stdin,
                },
                Response {
                    uri: website("http://example.org/redirect"),
                    status: Status::Redirected(http::StatusCode::PERMANENT_REDIRECT),
                    source: Input::Stdin,
                },
            ]
            .into_iter()
            .collect::<HashSet<_>>(),
        );
        assert_eq!(stats.fail_map, expected_map);
    }
}
