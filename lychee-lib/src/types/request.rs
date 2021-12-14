use std::{convert::TryFrom, fmt::Display};

use crate::{ErrorKind, Uri};

use super::InputSource;

/// A request type that can be handle by lychee
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Request {
    /// A valid Uniform Resource Identifier of a given endpoint, which can be
    /// checked with lychee
    pub uri: Uri,
    /// The resource which contained the given URI
    pub source: InputSource,
    /// Specifies where the link got rendered in a document
    /// This can be `img` or `a` but also `pre` or `code`.
    /// In case of plaintext input the field is `None`.
    pub attribute: Option<String>,
}

impl Request {
    /// Instantiate a new `Request` object
    #[inline]
    #[must_use]
    pub const fn new(uri: Uri, source: InputSource, attribute: Option<String>) -> Self {
        Request {
            uri,
            source,
            attribute,
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
            InputSource::String(uri.to_string()),
            None,
        ))
    }
}

impl TryFrom<String> for Request {
    type Error = ErrorKind;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let uri = Uri::try_from(s.as_str())?;
        Ok(Request::new(uri, InputSource::String(s), None))
    }
}

impl TryFrom<&str> for Request {
    type Error = ErrorKind;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let uri = Uri::try_from(s)?;
        Ok(Request::new(uri, InputSource::String(s.to_owned()), None))
    }
}
