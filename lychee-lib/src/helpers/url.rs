use linkify::LinkFinder;

use super::tlds::{TOP_TLDS, TOP_TLDS_CONTAINS};

/// Remove all GET parameters from a URL.
/// The link is not a URL but a String as it may not have a base domain.
pub(crate) fn remove_get_params_and_fragment(url: &str) -> &str {
    let path = match url.split_once('#') {
        Some((path_without_fragment, _fragment)) => path_without_fragment,
        None => url,
    };
    let path = match path.split_once('?') {
        Some((path_without_params, _params)) => path_without_params,
        None => path,
    };
    path
}

/// Use `LinkFinder` to offload the raw link searching in plaintext
///
/// If `no_scheme` is set to `true`, links without a scheme (e.g. without `https://`)
/// will also be extracted and returned.
pub(crate) fn find_links(input: &str, no_scheme: bool) -> impl Iterator<Item = linkify::Link> {
    let mut finder = LinkFinder::new();
    finder.url_must_have_scheme(!no_scheme);

    finder.links(input).into_iter().filter(|link| {
        let s = link.as_str();
        // Only pick URLs from top TLDs.
        // This is only a rudimentary check to keep the false-positive rate low.
        for tld in TOP_TLDS.iter() {
            if s.ends_with(tld) {
                return true;
            }
        }
        for tld in TOP_TLDS_CONTAINS.iter() {
            if s.contains(tld) {
                return true;
            }
        }
        false
    })
}

#[cfg(test)]
mod test_fs_tree {
    use super::*;

    #[test]
    fn test_remove_get_params_and_fragment() {
        assert_eq!(remove_get_params_and_fragment("/"), "/");
        assert_eq!(
            remove_get_params_and_fragment("index.html?foo=bar"),
            "index.html"
        );
        assert_eq!(
            remove_get_params_and_fragment("/index.html?foo=bar"),
            "/index.html"
        );
        assert_eq!(
            remove_get_params_and_fragment("/index.html?foo=bar&baz=zorx?bla=blub"),
            "/index.html"
        );
        assert_eq!(
            remove_get_params_and_fragment("https://example.com/index.html?foo=bar"),
            "https://example.com/index.html"
        );
        assert_eq!(
            remove_get_params_and_fragment("test.png?foo=bar"),
            "test.png"
        );

        assert_eq!(
            remove_get_params_and_fragment("https://example.com/index.html#anchor"),
            "https://example.com/index.html"
        );
        assert_eq!(
            remove_get_params_and_fragment("https://example.com/index.html?foo=bar#anchor"),
            "https://example.com/index.html"
        );
        assert_eq!(
            remove_get_params_and_fragment("test.png?foo=bar#anchor"),
            "test.png"
        );
        assert_eq!(
            remove_get_params_and_fragment("test.png#anchor?anchor!?"),
            "test.png"
        );
        assert_eq!(
            remove_get_params_and_fragment("test.png?foo=bar#anchor?anchor!"),
            "test.png"
        );
    }
}
