use std::borrow::Cow;
use std::sync::LazyLock;

use url::Url;

use linkify::LinkFinder;
use url::ParseError;

/// Returns whether the text represents a relative link that is
/// relative to the domain root. Textually, it looks like `/this`.
pub(crate) fn is_root_relative(text: &str) -> bool {
    let text = text.trim_ascii_start();
    text.starts_with('/') && !text.starts_with("//")
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
            if url.scheme() == "file" && is_root_relative(subpath) {
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
        Ok(url) if cfg!(windows) && url.scheme().len() == 1 => Err(input),
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

    #[test]
    fn test_join_rooted() {
        let test_urls_and_expected = [
            // normal HTTP traversal and parsing absolute links
            ("https://a.com/b", vec!["x/", "d"], "https://a.com/x/d"),
            ("https://a.com/b/", vec!["x/", "d"], "https://a.com/b/x/d"),
            (
                "https://a.com/b/",
                vec!["https://new.com", "d"],
                "https://new.com/d",
            ),
            // parsing absolute file://
            ("https://a.com/b/", vec!["file:///a", "d"], "file:///d"),
            ("https://a.com/b/", vec!["file:///a/", "d"], "file:///a/d"),
            (
                "https://a.com/b/",
                vec!["file:///a/b/", "../.."],
                "file:///",
            ),
            // file traversal
            ("file:///a/b/", vec!["/x/y"], "file:///a/b/x/y"),
            ("file:///a/b/", vec!["a/"], "file:///a/b/a/"),
            ("file:///a/b/", vec!["a/", "../.."], "file:///a/"),
            ("file:///a/b/", vec!["a/", "/"], "file:///a/b/"),
            ("file:///a/b/", vec!["/.."], "file:///a/"),
            ("file:///a/b/", vec!["/../../"], "file:///"),
            ("file:///a/b/", vec![""], "file:///a/b/"),
            ("file:///a/b/", vec!["."], "file:///a/b/"),
            // HTTP relative links
            ("https://a.com/x", vec![""], "https://a.com/x"),
            ("https://a.com/x", vec!["../../.."], "https://a.com/"),
            ("https://a.com/x", vec!["?q", "#x"], "https://a.com/x?q#x"),
            ("https://a.com/x", vec![".", "?a"], "https://a.com/?a"),
            ("https://a.com/x", vec!["/"], "https://a.com/"),
            ("https://a.com/x?q#anchor", vec![""], "https://a.com/x?q"),
            ("https://a.com/x#anchor", vec!["?x"], "https://a.com/x?x"),
            // scheme relative link - can traverse outside of root
            ("file:///root/", vec!["///new-root"], "file:///new-root"),
            ("file:///root/", vec!["//a.com/boop"], "file://a.com/boop"),
            ("https://root/", vec!["//a.com/boop"], "https://a.com/boop"),
        ];

        for (base, subpaths, expected) in test_urls_and_expected {
            println!("base={base}, subpaths={subpaths:?}, expected={expected}");
            assert_eq!(
                Url::parse(base)
                    .unwrap()
                    .join_rooted(&subpaths[..])
                    .unwrap()
                    .to_string(),
                expected
            );
        }
    }

    #[test]
    #[ignore = "katrinafyi: suspected bug with Url::make_relative"]
    fn test_join_rooted_with_trailing_filename() {
        let test_urls_and_expected = [
            // file URLs without trailing / are kinda weird.
            ("file:///a/b/c", vec!["/../../a"], "file:///a/b/a"),
            ("file:///a/b/c", vec!["/"], "file:///a/b/"),
            ("file:///a/b/c", vec![".?qq"], "file:///a/b/?qq"),
            ("file:///a/b/c", vec!["#x"], "file:///a/b/c#x"),
            ("file:///a/b/c", vec!["./"], "file:///a/b/"),
            ("file:///a/b/c", vec!["c"], "file:///a/b/c"),
            // joining with d
            ("file:///a/b/c", vec!["d", "/../../a"], "file:///a/b/a"),
            ("file:///a/b/c", vec!["d", "/"], "file:///a/b/"),
            ("file:///a/b/c", vec!["d", "."], "file:///a/b/"),
            ("file:///a/b/c", vec!["d", "./"], "file:///a/b/"),
            // joining with d/
            ("file:///a/b/c", vec!["d/", "/"], "file:///a/b/"),
            ("file:///a/b/c", vec!["d/", "."], "file:///a/b/d/"),
            ("file:///a/b/c", vec!["d/", "./"], "file:///a/b/d/"),
        ];

        for (base, subpaths, expected) in test_urls_and_expected {
            println!("base={base}, subpaths={subpaths:?}, expected={expected}");
            assert_eq!(
                Url::parse(base)
                    .unwrap()
                    .join_rooted(&subpaths[..])
                    .unwrap()
                    .to_string(),
                expected
            );
        }
    }
}
