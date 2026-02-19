use std::fmt::Display;

use http::StatusCode;
use serde::Serialize;

use crate::{InputSource, Status, Uri, types::uri::raw::RawUriSpan};

/// Response type returned by lychee after checking a URI
//
// Body is public to allow inserting into stats maps (error_map, success_map,
// etc.) without `Clone`, because the inner `ErrorKind` in `response.status` is
// not `Clone`. Use `body()` to access the body in the rest of the code.
//
// `pub(crate)` is insufficient, because the `stats` module is in the `bin`
// crate crate.
#[derive(Debug)]
pub struct Response {
    input_source: InputSource,

    /// TODO
    pub response_body: ResponseBody,
}

impl Response {
    #[inline]
    #[must_use]
    /// Create new response
    pub const fn new(
        uri: Uri,
        status: Status,
        input_source: InputSource,
        span: Option<RawUriSpan>,
    ) -> Self {
        Response {
            input_source,
            response_body: ResponseBody { uri, status, span },
        }
    }

    #[inline]
    #[must_use]
    /// Retrieve the underlying status of the response
    pub const fn status(&self) -> &Status {
        &self.response_body.status
    }

    #[inline]
    #[must_use]
    /// Retrieve the underlying source of the response
    /// (e.g. the input file or the URL)
    pub const fn source(&self) -> &InputSource {
        &self.input_source
    }

    #[inline]
    #[must_use]
    /// Retrieve the underlying body of the response
    pub const fn body(&self) -> &ResponseBody {
        &self.response_body
    }
}

impl Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <ResponseBody as Display>::fmt(&self.response_body, f)
    }
}

impl Serialize for Response {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        <ResponseBody as Serialize>::serialize(&self.response_body, s)
    }
}

/// Encapsulates the state of a URI check
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Serialize, Hash, PartialEq, Eq)]
pub struct ResponseBody {
    #[serde(flatten)]
    /// The URI which was checked
    pub uri: Uri,
    /// The status of the check
    pub status: Status,
    /// The location of the URI
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<RawUriSpan>,
}

// Extract as much information from the underlying error conditions as possible
// without being too verbose. Some dependencies (rightfully) don't expose all
// error fields to downstream crates, which is why we have to defer to pattern
// matching in these cases.
impl Display for ResponseBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Always write the URI
        write!(f, "{}", self.uri)?;

        if let Some(span) = self.span {
            write!(f, " at {span}")?;
        }

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
