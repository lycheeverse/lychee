use std::{convert::TryFrom, fmt::Display};

use crate::{ErrorKind, Input, Uri};

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Request {
    pub uri: Uri,
    pub source: Input,
}

impl Request {
    #[inline]
    #[must_use]
    pub fn new(uri: Uri, source: Input) -> Self {
        Request { uri, source }
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
        Ok(Request::new(uri, Input::String(s)))
    }
}

impl TryFrom<&str> for Request {
    type Error = ErrorKind;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let uri = Uri::try_from(s)?;
        Ok(Request::new(uri, Input::String(s.to_owned())))
    }
}
