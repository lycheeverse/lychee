use std::{convert::TryFrom, fmt::Display};

use crate::{ErrorKind, Input, Uri};

/// A request type that can be handle by lychee
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Request {
    /// A valid Uniform Resource Identifier of a given endpoint, which can be
    /// checked with lychee
    pub uri: Uri,
    /// The resource which contained the given URI
    pub source: Input,
    /// Specifies where the link got rendered in a document
    /// This can be `img` or `a` but also `pre` or `code`.
    /// In case of plaintext input the field is `None`.
    pub attribute: Option<String>,
}

impl Request {
    /// Instantiate a new `Request` object
    #[inline]
    #[must_use]
    pub const fn new(uri: Uri, source: Input, attribute: Option<String>) -> Self {
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

impl TryFrom<String> for Request {
    type Error = ErrorKind;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let uri = Uri::try_from(s.as_str())?;
        Ok(Request::new(uri, Input::String(s), None))
    }
}

impl TryFrom<&str> for Request {
    type Error = ErrorKind;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let uri = Uri::try_from(s)?;
        Ok(Request::new(uri, Input::String(s.to_owned()), None))
    }
}
