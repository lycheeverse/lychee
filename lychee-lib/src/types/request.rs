use std::{convert::TryFrom, fmt::Display};

use crate::{BasicAuthCredentials, ErrorKind, Uri};

use super::InputSource;

/// A request type that can be handle by lychee
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Request {
    /// A valid Uniform Resource Identifier of a given endpoint, which can be
    /// checked with lychee
    pub uri: Uri,

    /// The resource which contained the given URI
    pub source: InputSource,

    /// Specifies how the URI was rendered inside a document
    /// (for example `img`, `a`, `pre`, or `code`).
    /// In case of plaintext input the field is `None`.
    pub element: Option<String>,

    /// Specifies the attribute (e.g. `href`) that contained the URI
    pub attribute: Option<String>,

    /// Basic auth credentials
    pub credentials: Option<BasicAuthCredentials>,
}

impl Request {
    /// Instantiate a new `Request` object
    #[inline]
    #[must_use]
    pub const fn new(
        uri: Uri,
        source: InputSource,
        element: Option<String>,
        attribute: Option<String>,
        credentials: Option<BasicAuthCredentials>,
    ) -> Self {
        Request {
            uri,
            source,
            element,
            attribute,
            credentials,
        }
    }
}

impl Display for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.uri, self.source)
    }
}

impl TryFrom<Uri> for Request {
    type Error = ErrorKind;

    fn try_from(uri: Uri) -> Result<Self, Self::Error> {
        Ok(Request::new(
            uri.clone(),
            InputSource::RemoteUrl(Box::new(uri.url)),
            None,
            None,
            None,
        ))
    }
}

impl TryFrom<String> for Request {
    type Error = ErrorKind;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let uri = Uri::try_from(s.as_str())?;
        Ok(Request::new(uri, InputSource::String(s), None, None, None))
    }
}

impl TryFrom<&str> for Request {
    type Error = ErrorKind;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let uri = Uri::try_from(s)?;
        Ok(Request::new(
            uri,
            InputSource::String(s.to_owned()),
            None,
            None,
            None,
        ))
    }
}
