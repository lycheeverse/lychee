use crate::types::uri::relative::RelativeUri;
use crate::{ErrorKind, Uri};

use url::ParseError;

/// The result of parsing a string that could be either a full URL
/// or a relative path/fragment.
pub enum ParsedUri<'a> {
    /// A fully-qualified, absolute URI.
    Absolute(Uri),
    /// A relative URI that requires a base for resolution.
    Relative(RelativeUri<'a>),
}

impl<'a> ParsedUri<'a> {
    /// Attempts to parse the given text as either an absolute or relative
    /// link.
    ///
    /// # Errors
    ///
    /// Returns an error if the text cannot be parsed as a URL, and the parse error
    /// was not due to "relative link without base".
    pub fn parse(text: &'a str) -> Result<Self, ErrorKind> {
        let text = text.trim_ascii_start();

        match Uri::try_from(text) {
            Ok(uri) => Ok(ParsedUri::Absolute(uri)),
            Err(ErrorKind::ParseUrl(ParseError::RelativeUrlWithoutBase, _)) => {
                Ok(ParsedUri::Relative(RelativeUri::parse(text)))
            }
            Err(e) => Err(e),
        }
    }
}

impl<'a> TryFrom<&'a str> for ParsedUri<'a> {
    type Error = ErrorKind;

    fn try_from(text: &'a str) -> Result<Self, Self::Error> {
        ParsedUri::parse(text)
    }
}
