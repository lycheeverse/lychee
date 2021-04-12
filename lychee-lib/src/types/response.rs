use std::fmt::Display;

use serde::Serialize;

use crate::{Input, Status, Uri};

/// Response type returned by lychee after checking a URI
#[derive(Debug)]
pub struct Response(pub Input, pub ResponseBody);

impl Response {
    #[inline]
    #[must_use]
    /// Create new response
    pub const fn new(uri: Uri, status: Status, source: Input) -> Self {
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

impl Display for ResponseBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ResponseBody {
            ref uri,
            ref status,
        } = self;

        // TODO: Other errors?
        let metadata = match status {
            Status::Ok(code) | Status::Redirected(code) => {
                format!(" [{}]", code)
            }
            Status::Timeout(Some(code)) => format!(" [{}]", code),
            Status::Error(e) => format!(" ({})", e),
            _ => "".into(),
        };
        write!(f, "{} {}{}", status.icon(), uri, metadata)
    }
}
