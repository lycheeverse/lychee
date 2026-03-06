use std::sync::LazyLock;

use linkify::LinkFinder;
use url::Url;

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
