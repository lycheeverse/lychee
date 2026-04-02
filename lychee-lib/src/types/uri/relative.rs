/// A relative link. Leading whitespace is removed from the
/// contained [`str`] reference.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RelativeUri<'a> {
    /// A root-relative link, e.g. `"/help"`. The contained string will
    /// start with `/` and not start with `//`.
    ///
    /// This can also be called "domain-relative URL" (by [MDN]) and
    /// "path-absolute-URL string" (by [WHATWG]). From [MDN]:
    ///
    /// > Domain-relative URL: `/en-US/docs/Learn_web_development` — the protocol and
    /// > the domain name are both missing. The browser will use the same protocol
    /// > and the same domain name as the one used to load the document hosting that URL.
    ///
    /// [MDN]: https://developer.mozilla.org/en-US/docs/Learn_web_development/Howto/Web_mechanics/What_is_a_URL#absolute_urls_vs._relative_urls
    /// [WHATWG]: https://url.spec.whatwg.org/#path-absolute-url-string
    Root(&'a str),
    /// A scheme-relative link, e.g. `"//example.com/help"`. The contained
    /// string will start with `//`.
    ///
    /// From [MDN]:
    ///
    /// > Scheme-relative URL: `//developer.mozilla.org/en-US/docs/Learn_web_development` —
    /// > only the protocol is missing. The browser will use the same protocol as the one
    /// > used to load the document hosting that URL.
    ///
    /// [MDN]: https://developer.mozilla.org/en-US/docs/Learn_web_development/Howto/Web_mechanics/What_is_a_URL#absolute_urls_vs._relative_urls
    Scheme(&'a str),
    /// A locally-relative link. This is much less constrained than the other
    /// two variants. For example, `"help"`, `"../home"`, and `""` (the empty string)
    /// are all valid locally-relative links.
    Local(&'a str),
}

impl RelativeUri<'_> {
    /// Parses the given text as a [`RelativeUri`].
    ///
    /// Determining between [`RelativeUri::Root`] and [`RelativeUri::Scheme`]
    /// is done based on how many initial slashes are in the text. If there
    /// are *no* initial slashes, the text is assumed to be a [`RelativeUri::Local`].
    pub fn parse(text: &str) -> RelativeUri<'_> {
        let text = text.trim_ascii_start();

        // important to check for scheme-rel before root-rel, as both of them
        // start with a slash.
        if text.starts_with("//") {
            RelativeUri::Scheme(text)
        } else if text.starts_with('/') {
            RelativeUri::Root(text)
        } else {
            RelativeUri::Local(text)
        }
    }

    /// Returns the string text of the given relative link. The returned
    /// string has leading whitespace trimmed.
    pub const fn link_text(&self) -> &str {
        match self {
            Self::Root(x) | Self::Scheme(x) | Self::Local(x) => x,
        }
    }
}
