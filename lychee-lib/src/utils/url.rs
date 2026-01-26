use std::borrow::Cow;
use std::sync::LazyLock;

use url::Url;

use linkify::LinkFinder;
use url::ParseError;

pub(crate) trait ReqwestUrlExt {
    /// Joins the given subpaths, using the current URL as the base URL.
    ///
    /// Conceptually, `url.join_rooted(&[path])` is very similar to
    /// `url.join(path)` (using [`Url::join`]). However, they differ when
    /// the base URL is a `file:` URL.
    ///
    /// When used with a `file:` base URL, [`ReqwestUrlExt::join_rooted`]
    /// will ensure that any relative links will *not* traverse outside
    /// of the given base URL. In this way, it is "rooted" at the `file:`
    /// base URL.
    ///
    /// Note that this rooting behaviour only happens for `file:` bases.
    /// Relative links with non-`file:` bases can traverse anywhere as
    /// usual.
    fn join_rooted(&self, subpaths: &[&str]) -> Result<Url, ParseError>;
}

impl ReqwestUrlExt for Url {
    fn join_rooted(&self, subpaths: &[&str]) -> Result<Url, ParseError> {
        let base = self;

        // for file:// base URLs, we need to apply *rooting* and make sure
        // we don't go outside of the base.
        //
        // the idea is to make a "fake" base at the filesystem root, so
        // excessive ".." links will get absorbed and have no effect.
        //
        // we need some extra bookkeeping to detect when this base was used
        // and maintain a filename
        let fake_base = match base.scheme() {
            "file" => {
                let mut fake_base = base.join("/")?;
                fake_base.set_host(Some("secret-lychee-base-url.invalid"))?;

                let mut filename = base
                    .path_segments()
                    .and_then(|mut x| x.next_back())
                    .unwrap_or(".")
                    .to_string();

                if let Some(query) = base.query() {
                    filename.push('?');
                    filename.push_str(query);
                }

                fake_base = fake_base.join(&filename)?;

                Some(fake_base)
            }
            _ => None,
        };

        let mut url = Cow::Borrowed(fake_base.as_ref().unwrap_or(base));
        for subpath in subpaths {
            url = Cow::Owned(url.join(subpath)?);
        }

        match fake_base.as_ref().and_then(|b| b.make_relative(&url)) {
            Some(relative_to_base) => base.join(&relative_to_base),
            None => Ok(url.into_owned()),
        }
    }
}

/// Attempts to parse a string which may be a URL or a filesystem path.
/// Returns [`Ok`] if it is a valid URL, or [`Err`] if it is a filesystem path.
///
/// On Windows, we take care to make sure absolute paths---which could also be
/// parsed as URLs---are returned as filesystem paths.
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
            // file traversal - should stay within root
            ("file:///a/b/", vec!["a/"], "file:///a/b/a/"),
            ("file:///a/b/", vec!["a/", "../.."], "file:///a/b/"),
            ("file:///a/b/", vec!["a/", "/"], "file:///a/b/"),
            ("file:///a/b/", vec!["/.."], "file:///a/b/"),
            ("file:///a/b/", vec!["/../../"], "file:///a/b/"),
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
