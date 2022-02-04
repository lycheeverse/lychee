use std::fmt::Display;

use serde::Serialize;

use crate::{InputSource, Status, Uri};

use super::RecursionLevel;

/// Response type returned by lychee after checking a URI
#[derive(Debug)]
pub struct Response(pub InputSource, pub ResponseBody, pub RecursionLevel);

impl Response {
    #[inline]
    #[must_use]
    /// Create new response
    pub const fn new(uri: Uri, status: Status, source: InputSource) -> Self {
        Response(source, ResponseBody { uri, status }, 0)
    }

    /// Create new response with given recursion level
    #[inline]
    #[must_use]
    pub const fn with_recursion(
        uri: Uri,
        status: Status,
        source: InputSource,
        recursion_level: RecursionLevel,
    ) -> Self {
        Response(source, ResponseBody { uri, status }, recursion_level)
    }

    #[inline]
    #[must_use]
    /// Retrieve the underlying status of the response
    pub const fn status(&self) -> &Status {
        &self.1.status
    }

    #[inline]
    #[must_use]
    /// Convenience method to get the input source
    pub const fn source(&self) -> &InputSource {
        &self.0
    }

    #[inline]
    #[must_use]
    /// Convenience method to check if a request was successful
    pub const fn is_success(&self) -> bool {
        self.1.status.is_success()
    }

    #[inline]
    #[must_use]
    /// Convenience method to check if a request was cached
    pub const fn is_cached(&self) -> bool {
        self.1.status.is_cached()
    }

    /// Convenience method to get the response URI
    #[must_use]
    pub fn uri(&self) -> Uri {
        self.1.uri.clone()
    }

    /// Get recursion level of request
    #[must_use]
    pub const fn recursion_level(&self) -> RecursionLevel {
        self.2
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
        write!(f, "{} {}", self.status.icon(), self.uri)?;

        // TODO: Other errors?
        match &self.status {
            Status::Ok(code) | Status::Redirected(code) => {
                write!(f, " [{}]", code)
            }
            Status::Timeout(Some(code)) => write!(f, " [{}]", code),
            Status::Error(e) => write!(f, ": {}", e),
            _ => Ok(()),
        }
    }
}
