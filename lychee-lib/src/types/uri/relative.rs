use either::{Either, Left, Right};
use url::ParseError;

use crate::ErrorKind;
use crate::Uri;

/// A relative link text fragment. Leading whitespace is removed from the
/// string reference.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RelativeUri<'a> {
    /// A root-relative link, e.g. `"/help"`. The contained string will
    /// start with `/` and not start with `//`.
    Root(&'a str),
    /// A scheme-relative link, e.g. `"//example.com/help"`. The contained
    /// string will start with `//`.
    Scheme(&'a str),
    /// A locally-relative link, e.g. `"help"` or `"../home"`.
    Local(&'a str),
}

impl RelativeUri<'_> {
    /// Returns the string text of the given relative link. The returned
    /// string has leading whitespace trimmed.
    pub const fn link_text(&self) -> &str {
        match self {
            Self::Root(x) | Self::Scheme(x) | Self::Local(x) => x,
        }
    }

    /// Interprets the current relative link as a locally-relative link
    /// and returns the link text.
    ///
    /// For a root-relative link like `/rest`, this will return `./rest`.
    /// For a scheme-relative link like `//rest`, this will return
    /// `./rest`. A locally-relative link will be returned unchanged.
    ///
    /// This is occasionally useful - for example, to resolve root-relative
    /// links to a known local root directory.
    pub fn to_local_link_text(&self) -> String {
        match self {
            Self::Local(x) => x.to_string(),
            Self::Root(x) => format!(".{x}"),
            Self::Scheme(x) => {
                let x = &x[1..];
                format!(".{x}")
            }
        }
    }
}

/// Returns whether the text represents a root-relative link. These look like
/// `/this` and are resolved relative to a base URL's origin. This can also be called
/// "domain-relative URL" (by [MDN]) and "path-absolute-URL string" (by [WHATWG]).
/// From [MDN]:
///
/// > Domain-relative URL: `/en-US/docs/Learn_web_development` — the protocol and
/// > the domain name are both missing. The browser will use the same protocol
/// > and the same domain name as the one used to load the document hosting that URL.
///
/// [MDN]: https://developer.mozilla.org/en-US/docs/Learn_web_development/Howto/Web_mechanics/What_is_a_URL#absolute_urls_vs._relative_urls
/// [WHATWG]: https://url.spec.whatwg.org/#path-absolute-url-string
pub(crate) fn is_root_relative_link(text: &str) -> bool {
    !is_scheme_relative_link(text) && text.trim_ascii_start().starts_with('/')
}

/// Returns whether the text represents a scheme-relative link. These look like
/// `//example.com/subpath`. From [MDN]:
///
/// > Scheme-relative URL: `//developer.mozilla.org/en-US/docs/Learn_web_development` —
/// > only the protocol is missing. The browser will use the same protocol as the one
/// > used to load the document hosting that URL.
///
/// [MDN]: https://developer.mozilla.org/en-US/docs/Learn_web_development/Howto/Web_mechanics/What_is_a_URL#absolute_urls_vs._relative_urls
pub(crate) fn is_scheme_relative_link(text: &str) -> bool {
    text.trim_ascii_start().starts_with("//")
}

/// Attempts to parse the given text into a [`Uri`] or [`RelativeUri`].
///
/// # Errors
///
/// Returns an error if the text cannot be parsed as a URL, and the parse error
/// was not due to "relative link without base".
pub fn parse_url_or_relative(text: &str) -> Result<Either<Uri, RelativeUri<'_>>, ErrorKind> {
    let text = text.trim_ascii_start();

    match Uri::try_from(text) {
        Ok(uri) => Ok(Left(uri)),

        Err(ErrorKind::ParseUrl(ParseError::RelativeUrlWithoutBase, _)) => {
            if is_scheme_relative_link(text) {
                Ok(Right(RelativeUri::Scheme(text)))
            } else if is_root_relative_link(text) {
                Ok(Right(RelativeUri::Root(text)))
            } else {
                Ok(Right(RelativeUri::Local(text)))
            }
        }
        Err(e) => Err(e),
    }
}
