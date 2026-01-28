use std::borrow::Cow;
use std::sync::LazyLock;

use linkify::LinkFinder;
use url::{ParseError, Url};

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

pub(crate) trait ReqwestUrlExt {
    /// Joins the given subpaths, using the current URL as the base URL.
    ///
    /// Conceptually, `url.join_rooted(&[path])` is very similar to
    /// `url.join(path)` (using [`Url::join`]). However, they differ when
    /// the base URL is a `file:` URL.
    ///
    /// When used with a `file:` base URL, [`ReqwestUrlExt::join_rooted`]
    /// will treat root-relative links as locally-relative links, relative
    /// to the `file:` base URL.
    ///
    /// Other relative links and links with non-`file:` bases are joined
    /// normally, matching the behaviour of [`Url::join`].
    fn join_rooted(&self, subpaths: &[&str]) -> Result<Url, ParseError>;
}

impl ReqwestUrlExt for Url {
    fn join_rooted(&self, subpaths: &[&str]) -> Result<Url, ParseError> {
        let mut url = Cow::Borrowed(self);

        for subpath in subpaths {
            if url.scheme() == "file" && is_root_relative_link(subpath) {
                let locally_relative = format!(".{}", subpath.trim_ascii_start());
                url = Cow::Owned(self.join(&locally_relative)?);
            } else {
                url = Cow::Owned(url.join(subpath)?);
            }
        }

        Ok(url.into_owned())
    }
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
    // normal HTTP traversal and parsing absolute links
    #[case::http1("https://a.com/b", &["x/", "d"], "https://a.com/x/d")]
    #[case::http2("https://a.com/b/", &["x/", "d"], "https://a.com/b/x/d")]
    #[case::http3("https://a.com/b/", &["https://new.com", "d"], "https://new.com/d")]
    // parsing absolute file://
    #[case::file_abs1("https://a.com/b/", &["file:///a", "d"], "file:///d")]
    #[case::file_abs2("https://a.com/b/", &["file:///a/", "d"], "file:///a/d")]
    #[case::file_abs3("https://a.com/b/", &["file:///a/b/", "../.."], "file:///")]
    // file traversal
    #[case::file_rel1("file:///a/b/", &["/x/y"], "file:///a/b/x/y")]
    #[case::file_rel2("file:///a/b/", &["a/"], "file:///a/b/a/")]
    #[case::file_rel3("file:///a/b/", &["a/", "../.."], "file:///a/")]
    #[case::file_rel4("file:///a/b/", &["a/", "/"], "file:///a/b/")]
    #[case::file_rel5("file:///a/b/", &["/.."], "file:///a/")]
    #[case::file_rel6("file:///a/b/", &["/../../"], "file:///")]
    #[case::file_rel7("file:///a/b/", &[""], "file:///a/b/")]
    #[case::file_rel8("file:///a/b/", &["."], "file:///a/b/")]
    // HTTP relative links
    #[case::http_rel1("https://a.com/x", &[""], "https://a.com/x")]
    #[case::http_rel2("https://a.com/x", &["../../.."], "https://a.com/")]
    #[case::http_rel3("https://a.com/x", &["?q", "#x"], "https://a.com/x?q#x")]
    #[case::http_rel4("https://a.com/x", &[".", "?a"], "https://a.com/?a")]
    #[case::http_rel5("https://a.com/x", &["/"], "https://a.com/")]
    #[case::http_rel6("https://a.com/x?q#anchor", &[""], "https://a.com/x?q")]
    #[case::http_rel7("https://a.com/x#anchor", &["?x"], "https://a.com/x?x")]
    // scheme relative link - can traverse outside of root
    #[case::scheme_rel1("file:///root/", &["///new-root"], "file:///new-root")]
    #[case::scheme_rel2("file:///root/", &["//a.com/boop"], "file://a.com/boop")]
    #[case::scheme_rel3("https://root/", &["//a.com/boop"], "https://a.com/boop")]
    fn test_join_rooted(#[case] base: &str, #[case] subpaths: &[&str], #[case] expected: &str) {
        println!("base={base}, subpaths={subpaths:?}, expected={expected}");
        assert_eq!(
            Url::parse(base)
                .unwrap()
                .join_rooted(subpaths)
                .unwrap()
                .to_string(),
            expected
        );
    }

    #[rstest]
    // file URLs without trailing / are kinda weird.
    #[case::file_rel1("file:///a/b/c", &["/../../x"], "file:///x")]
    #[case::file_rel2("file:///a/b/c", &["/"], "file:///a/b/")]
    #[case::file_rel3("file:///a/b/c", &[".?qq"], "file:///a/b/?qq")]
    #[case::file_rel4("file:///a/b/c", &["#x"], "file:///a/b/c#x")]
    #[case::file_rel5("file:///a/b/c", &["./"], "file:///a/b/")]
    #[case::file_rel6("file:///a/b/c", &["c"], "file:///a/b/c")]
    // joining with d
    #[case::file_rel_d1("file:///a/b/c", &["d", "/../../x"], "file:///x")]
    #[case::file_rel_d2("file:///a/b/c", &["d", "/"], "file:///a/b/")]
    #[case::file_rel_d3("file:///a/b/c", &["d", "."], "file:///a/b/")]
    #[case::file_rel_d4("file:///a/b/c", &["d", "./"], "file:///a/b/")]
    // joining with d/
    #[case::file_rel_d_slash1("file:///a/b/c", &["d/", "/"], "file:///a/b/")]
    #[case::file_rel_d_slash2("file:///a/b/c", &["d/", "."], "file:///a/b/d/")]
    #[case::file_rel_d_slash3("file:///a/b/c", &["d/", "./"], "file:///a/b/d/")]
    fn test_join_rooted_with_trailing_filename(
        #[case] base: &str,
        #[case] subpaths: &[&str],
        #[case] expected: &str,
    ) {
        println!("base={base}, subpaths={subpaths:?}, expected={expected}");
        assert_eq!(
            Url::parse(base)
                .unwrap()
                .join_rooted(subpaths)
                .unwrap()
                .to_string(),
            expected
        );
    }

    #[rstest]
    // definitely URLs
    #[case::ok1("tel:1", Ok("tel:1"))]
    #[case::ok2("file:///a", Ok("file:///a"))]
    #[case::ok3("http://a.com", Ok("http://a.com/"))]
    // path-looking things
    #[case::err1("", Err(""))]
    #[case::err2(".", Err("."))]
    #[case::err3("C:", Err("C:"))]
    #[case::err4("/unix", Err("/unix"))]
    #[case::err5("C:/a", Err("C:/a"))]
    #[case::err6(r"C:\a\b", Err(r"C:\a\b"))]
    #[case::err7("**/*.md", Err("**/*.md"))]
    #[case::err8("something", Err("something"))]
    fn test_parse_url_or_path(#[case] input: &str, #[case] expected: Result<&str, &str>) {
        let result = parse_url_or_path(input);
        assert_eq!(result.as_ref().map(Url::as_str), expected.as_deref());
    }
}
