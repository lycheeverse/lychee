use std::{borrow::Cow, convert::TryFrom, fmt::Display};

use crate::{BasicAuthCredentials, ErrorKind, Uri, types::uri::raw::RawUriSpan};

use super::ResolvedInputSource;

/// A checkable element extracted from a document by lychee,
/// containing a URI and its location within the source.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Request {
    /// The extracted Uniform Resource Identifier
    /// which can be checked with lychee
    pub uri: Uri,

    /// The resource which contains the given URI
    pub source: ResolvedInputSource,

    /// How the URI is rendered inside a document
    /// (for example `img`, `a`, `pre`, or `code`).
    /// In case of plaintext input the field is `None`.
    pub element: Option<String>,

    /// What attribute (e.g. `href`) the URI is contained in
    pub attribute: Option<String>,

    /// Where the URI is located
    pub span: Option<RawUriSpan>,

    /// Basic auth credentials
    pub credentials: Option<BasicAuthCredentials>,
}

impl Request {
    /// Instantiate a new `Request` object,
    /// with optional fields set to `None`
    #[inline]
    #[must_use]
    pub const fn new(uri: Uri, source: ResolvedInputSource) -> Self {
        Request {
            uri,
            source,
            element: None,
            attribute: None,
            span: None,
            credentials: None,
        }
    }

    /// Set [`Self::element`]
    #[must_use]
    pub fn with_element(mut self, element: String) -> Self {
        self.element = Some(element);
        self
    }

    /// Set [`Self::attribute`]
    #[must_use]
    pub fn with_attribute(mut self, attribute: String) -> Self {
        self.attribute = Some(attribute);
        self
    }

    /// Set [`Self::span`]
    #[must_use]
    pub const fn with_span(mut self, span: RawUriSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// Set [`Self::credentials`]
    #[must_use]
    pub fn with_credentials(mut self, credentials: BasicAuthCredentials) -> Self {
        self.credentials = Some(credentials);
        self
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
        ))
    }
}
