use std::{error::Error, fmt::Display};

use http::StatusCode;
use serde::Serialize;

use crate::{helpers, ErrorKind, InputSource, Status, Uri};

/// Response type returned by lychee after checking a URI
#[derive(Debug)]
pub struct Response(pub InputSource, pub ResponseBody);

impl Response {
    #[inline]
    #[must_use]
    /// Create new response
    pub const fn new(uri: Uri, status: Status, source: InputSource) -> Self {
        Response(source, ResponseBody { uri, status })
    }

    #[inline]
    #[must_use]
    /// Retrieve the underlying status of the response
    pub const fn status(&self) -> &Status {
        &self.1.status
    }
}

impl Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <ResponseBody as Display>::fmt(&self.1, f)
    }
}

impl Serialize for Response {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        <ResponseBody as Serialize>::serialize(&self.1, s)
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Serialize, Hash, PartialEq, Eq)]
/// Encapsulates the state of a URI check
pub struct ResponseBody {
    #[serde(flatten)]
    /// The URI which was checked
    pub uri: Uri,
    /// The status of the check
    pub status: Status,
}

// Extract as much information from the underlying error conditions as possible
// without being too verbose. Some dependencies (rightfully) don't expose all
// error fields to downstream crates, which is why we have to defer to pattern
// matching in these cases.
impl Display for ResponseBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} [{}] {}",
            self.status.icon(),
            self.status.code(),
            self.uri
        )?;

        if let Status::Ok(StatusCode::OK) = self.status {
            // Don't print anything else if the status code is 200.
            // The output gets too verbose then.
            return Ok(());
        }

        // Add a separator between the URI and the additional details below.
        // Note: To make the links clickable in some terminals,
        // we add a space before the separator.
        write!(f, " | ")?;

        match &self.status {
            Status::Ok(code) => write!(f, "{}", code.canonical_reason().unwrap_or("OK")),
            Status::Redirected(code) => {
                write!(f, "{}", code.canonical_reason().unwrap_or("Redirected"))
            }
            Status::Timeout(Some(code)) => write!(f, "Timeout [{code}]"),
            Status::Timeout(None) => write!(f, "Timeout"),
            Status::UnknownStatusCode(code) => write!(f, "Unknown status code [{code}]"),
            Status::Excluded => write!(f, "Excluded"),
            Status::Unsupported(e) => write!(f, "Unsupported {e}"),
            Status::Cached(status) => write!(f, "{status}"),
            Status::Error(e) => {
                let details = match e {
                    ErrorKind::NetworkRequest(e) => {
                        if let Some(status) = e.status() {
                            status
                                .canonical_reason()
                                .unwrap_or("Unknown status code")
                                .to_string()
                        } else {
                            helpers::reqwest::trim_error_output(e)
                        }
                    }
                    ErrorKind::GithubRequest(e) => match e {
                        octocrab::Error::GitHub { source, .. } => source.message.to_string(),
                        _ => "".to_string(),
                    },
                    _ => e.source().map_or("".to_string(), ToString::to_string),
                };
                if details.is_empty() {
                    write!(f, "{e}")
                } else {
                    write!(f, "{e}: {details}")
                }
            }
        }
    }
}
