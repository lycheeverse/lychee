use std::sync::LazyLock;

use linkify::LinkFinder;
use url::Url;

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

/// Attempts to parse a string which might represent a URL or a filesystem path.
/// Returns [`Ok`] if it is unambiguously a valid URL, otherwise returns [`Err`]
/// with the original input.
///
/// On Windows, we take care to make sure absolute paths---which could also be
/// parsed as URLs---are not parsed as URLs.
///
/// # Errors
///
/// Returns an [`Err`] if the given text is not a valid URL, or if the given text
/// *could* be interpreted as a filesystem path. The string is returned within
/// the error to allow for easier subsequent processing.
pub(crate) fn parse_url_or_path(input: &str) -> Result<Url, &str> {
    match Url::parse(input) {
        Ok(url) if url.scheme().len() == 1 => Err(input),
        Ok(url) => Ok(url),
        _ => Err(input),
    }
}

static LINK_FINDER: LazyLock<LinkFinder> = LazyLock::new(LinkFinder::new);

// Use `LinkFinder` to offload the raw link searching in plaintext
pub(crate) fn find_links(input: &str) -> impl Iterator<Item = linkify::Link<'_>> {
    LINK_FINDER.links(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    // OK URLs
    #[case("tel:1", Ok("tel:1"))]
    #[case("file:///a", Ok("file:///a"))]
    #[case("http://a.com", Ok("http://a.com/"))]
    // Invalid URLs
    #[case("", Err(""))]
    #[case(".", Err("."))]
    #[case("C:", Err("C:"))]
    #[case("/unix", Err("/unix"))]
    #[case("C:/a", Err("C:/a"))]
    #[case(r"C:\a\b", Err(r"C:\a\b"))]
    #[case("**/*.md", Err("**/*.md"))]
    #[case("something", Err("something"))]
    fn test_parse_url_or_path(#[case] input: &str, #[case] expected: Result<&str, &str>) {
        let result = parse_url_or_path(input);
        assert_eq!(
            result.as_ref().map(Url::as_str),
            expected.as_deref(),
            "input={input:?}, expected={expected:?}"
        );
    }
}
