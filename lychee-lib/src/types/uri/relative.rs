use either::{Either, Left, Right};
use url::ParseError;

use crate::ErrorKind;
use crate::Uri;

/// A relative link text fragment. Leading whitespace is removed from the
/// string reference.
#[derive(Debug, PartialEq, Eq, Clone)]
#[expect(
    clippy::enum_variant_names,
    reason = "suffixing with Rel makes the names more sensible to read"
)]
pub enum RelativeUri<'a> {
    /// A root-relative link, e.g. `"/help"`. The contained string will
    /// start with `/` and not start with `//`.
    RootRel(&'a str),
    /// A scheme-relative link, e.g. `"//example.comhelp"`. The contained
    /// string will start with `//`.
    SchemeRel(&'a str),
    /// A locally-relative link, e.g. `"help"` or `"../home"`.
    LocalRel(&'a str),
}

pub use RelativeUri::{LocalRel, RootRel, SchemeRel};

impl RelativeUri<'_> {
    /// Returns the string text of the given relative link. The returned
    /// string has leading whitespace trimmed.
    pub const fn link_text(&self) -> &str {
        match self {
            RootRel(x) | SchemeRel(x) | LocalRel(x) => x,
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

/// Attempts to parse the given text as a into a [`Uri`] or [`RelativeUri`].
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
                Ok(Right(SchemeRel(text)))
            } else if is_root_relative_link(text) {
                Ok(Right(RootRel(text)))
            } else {
                Ok(Right(LocalRel(text)))
            }
        }
        Err(e) => Err(e),
    }
}
