use std::sync::LazyLock;

use url::Url;

use linkify::LinkFinder;

static LINK_FINDER: LazyLock<LinkFinder> = LazyLock::new(LinkFinder::new);

/// Attempts to parse a string which may be a URL or a filesystem path.
/// Returns [`Ok`] if it is a valid URL, or [`Err`] if it is a filesystem path.
///
/// On Windows, we take care to make sure absolute paths---which could also be
/// parsed as URLs---are returned as filesystem paths.
pub(crate) fn parse_url_or_path(input: &str) -> Result<Url, String> {
    match Url::parse(input) {
        Ok(url) if cfg!(windows) && url.scheme().len() == 1 => Err(input.to_string()),
        Ok(url) => Ok(url),
        _ => Err(input.to_string()),
    }
}

/// Remove all GET parameters from a URL and separates out the fragment.
/// The link is not a URL but a String as it may not have a base domain.
pub(crate) fn remove_get_params_and_separate_fragment(url: &str) -> (&str, Option<&str>) {
    let (path, frag) = match url.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (url, None),
    };
    let path = match path.split_once('?') {
        Some((path_without_params, _params)) => path_without_params,
        None => path,
    };
    (path, frag)
}

// Use `LinkFinder` to offload the raw link searching in plaintext
pub(crate) fn find_links(input: &str) -> impl Iterator<Item = linkify::Link<'_>> {
    LINK_FINDER.links(input)
}

#[cfg(test)]
mod test_fs_tree {
    use super::*;

    #[test]
    fn test_remove_get_params_and_fragment() {
        assert_eq!(remove_get_params_and_separate_fragment("/"), ("/", None));
        assert_eq!(
            remove_get_params_and_separate_fragment("index.html?foo=bar"),
            ("index.html", None)
        );
        assert_eq!(
            remove_get_params_and_separate_fragment("/index.html?foo=bar"),
            ("/index.html", None)
        );
        assert_eq!(
            remove_get_params_and_separate_fragment("/index.html?foo=bar&baz=zorx?bla=blub"),
            ("/index.html", None)
        );
        assert_eq!(
            remove_get_params_and_separate_fragment("https://example.com/index.html?foo=bar"),
            ("https://example.com/index.html", None)
        );
        assert_eq!(
            remove_get_params_and_separate_fragment("test.png?foo=bar"),
            ("test.png", None)
        );

        assert_eq!(
            remove_get_params_and_separate_fragment("https://example.com/index.html#anchor"),
            ("https://example.com/index.html", Some("anchor"))
        );
        assert_eq!(
            remove_get_params_and_separate_fragment(
                "https://example.com/index.html?foo=bar#anchor"
            ),
            ("https://example.com/index.html", Some("anchor"))
        );
        assert_eq!(
            remove_get_params_and_separate_fragment("test.png?foo=bar#anchor"),
            ("test.png", Some("anchor"))
        );
        assert_eq!(
            remove_get_params_and_separate_fragment("test.png#anchor?anchor!?"),
            ("test.png", Some("anchor?anchor!?"))
        );
        assert_eq!(
            remove_get_params_and_separate_fragment("test.png?foo=bar#anchor?anchor!"),
            ("test.png", Some("anchor?anchor!"))
        );
    }
}
