use std::convert::TryFrom;
use std::sync::LazyLock;
use thiserror::Error;

use crate::{ErrorKind, RawUri, Response, Status, Uri};
use crate::{InputSource, ResolvedInputSource};

static ERROR_URI: LazyLock<Uri> = LazyLock::new(|| Uri::try_from("error:").unwrap());

/// An error which occurs while trying to construct a [`Request`] object.
/// That is, an error which happens while trying to load links from an input
/// source.
#[derive(Error, Debug, PartialEq, Eq, Hash)]
pub enum RequestError {
    /// Unable to construct a URL for a link appearing within the given source.
    #[error("Error building URL for {0}: {2}")]
    CreateRequestItem(RawUri, ResolvedInputSource, #[source] ErrorKind),

    /// Unable to load the content of an input source.
    #[error("Error reading input '{0}': {1}")]
    GetInputContent(InputSource, #[source] ErrorKind),

    /// Unable to load an input source directly specified by the user.
    #[error("Error reading user input '{0}': {1}")]
    UserInputContent(InputSource, #[source] ErrorKind),
}

impl RequestError {
    /// Get the underlying cause of this [`RequestError`].
    #[must_use]
    pub const fn error(&self) -> &ErrorKind {
        match self {
            Self::CreateRequestItem(_, _, e)
            | Self::GetInputContent(_, e)
            | Self::UserInputContent(_, e) => e,
        }
    }

    /// Convert this [`RequestError`] into its source error.
    #[must_use]
    pub fn into_error(self) -> ErrorKind {
        match self {
            Self::CreateRequestItem(_, _, e)
            | Self::GetInputContent(_, e)
            | Self::UserInputContent(_, e) => e,
        }
    }

    /// Get (a clone of) the input source within which the error happened.
    #[must_use]
    pub fn input_source(&self) -> InputSource {
        match self {
            Self::CreateRequestItem(_, src, _) => src.clone().into(),
            Self::GetInputContent(src, _) | Self::UserInputContent(src, _) => src.clone(),
        }
    }

    /// Convert this request error into a (failed) [`Response`] for reporting
    /// purposes.
    ///
    /// # Errors
    ///
    /// If this `RequestError` was caused by failing to load a user-specified
    /// input, the underlying cause of the `RequestError` will be returned
    /// as an Err. This allows the error to be propagated back to the user.
    pub fn into_response(self) -> Result<Response, ErrorKind> {
        match self {
            RequestError::UserInputContent(_, e) => Err(e),
            e => {
                let src = e.input_source();
                Ok(Response::new(
                    ERROR_URI.clone(),
                    Status::RequestError(e),
                    src,
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ERROR_URI;
    use std::sync::LazyLock;

    #[test]
    fn test_error_url_parses() {
        let _ = LazyLock::force(&ERROR_URI);
    }
}
