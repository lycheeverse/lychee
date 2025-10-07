use std::{borrow::Cow, convert::TryFrom, fmt::Display};
use thiserror::Error;

use crate::{BasicAuthCredentials, ErrorKind, RawUri, Uri};
use crate::{InputSource, ResolvedInputSource};

/// An error which occurs while trying to construct a [`Request`] object.
/// That is, an error which happens while trying to load links from an input
/// source.
#[derive(Error, Debug, PartialEq, Eq, Hash)]
pub enum RequestError {
    /// Unable to construct a URL for a link appearing within the given source.
    #[error("Error building URL for {0}: {2}")]
    CreateRequestItem(RawUri, ResolvedInputSource, #[source] Box<ErrorKind>),

    /// Unable to load the content of an input source.
    #[error("Error reading input '{0}': {1}")]
    GetInputContent(InputSource, #[source] Box<ErrorKind>),
}

impl RequestError {
    /// Get the underlying cause of this [`RequestError`].
    #[must_use]
    pub const fn error(&self) -> &ErrorKind {
        match self {
            Self::CreateRequestItem(_, _, e) | Self::GetInputContent(_, e) => e,
        }
    }

    /// Convert this [`RequestError`] into its source error.
    #[must_use]
    pub fn into_error(self) -> ErrorKind {
        match self {
            Self::CreateRequestItem(_, _, e) | Self::GetInputContent(_, e) => *e,
        }
    }
}

/// A request type that can be handle by lychee
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Request {
    /// A valid Uniform Resource Identifier of a given endpoint, which can be
    /// checked with lychee
    pub uri: Uri,

    /// The resource which contained the given URI
    pub source: ResolvedInputSource,

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
        source: ResolvedInputSource,
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
            ResolvedInputSource::RemoteUrl(Box::new(uri.url)),
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
        Ok(Request::new(
            uri,
            ResolvedInputSource::String(Cow::Owned(s)),
            None,
            None,
            None,
        ))
    }
}

impl TryFrom<&str> for Request {
    type Error = ErrorKind;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let uri = Uri::try_from(s)?;
        Ok(Request::new(
            uri,
            ResolvedInputSource::String(Cow::Owned(s.to_owned())),
            None,
            None,
            None,
        ))
    }
}
