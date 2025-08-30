use std::fmt::Display;

use http::StatusCode;
use serde::Serialize;

use crate::{InputSource, Status, Uri};
use crate::types::ResolvedInputSource;

/// Response type returned by lychee after checking a URI
//
// Body is public to allow inserting into stats maps (error_map, success_map,
// etc.) without `Clone`, because the inner `ErrorKind` in `response.status` is
// not `Clone`. Use `body()` to access the body in the rest of the code.
//
// `pub(crate)` is insufficient, because the `stats` module is in the `bin`
// crate crate.
#[derive(Debug)]
pub struct Response(ResolvedInputSource, pub ResponseBody);

impl Response {
    #[inline]
    #[must_use]
    /// Create new response
    pub const fn new(uri: Uri, status: Status, source: ResolvedInputSource) -> Self {
        Response(source, ResponseBody { uri, status })
    }

    #[inline]
    #[must_use]
    /// Retrieve the underlying status of the response
    pub const fn status(&self) -> &Status {
        &self.1.status
    }

    #[inline]
    #[must_use]
    /// Retrieve the underlying source of the response
    /// (e.g. the input file or the URL)
    pub const fn source(&self) -> &ResolvedInputSource {
        &self.0
    }

    #[inline]
    #[must_use]
    /// Retrieve the underlying body of the response
    pub const fn body(&self) -> &ResponseBody {
        &self.1
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
        // Always write the URI
        write!(f, "{}", self.uri)?;

        // Early return for OK status to avoid verbose output
        if matches!(self.status, Status::Ok(StatusCode::OK)) {
            return Ok(());
        }

        // Format status and return early if empty
        let status_output = self.status.to_string();
        if status_output.is_empty() {
            return Ok(());
        }

        // Write status with separator
        write!(f, " | {status_output}")?;

        // Add details if available
        if let Some(details) = self.status.details() {
            write!(f, ": {details}")
        } else {
            Ok(())
        }
    }
}
