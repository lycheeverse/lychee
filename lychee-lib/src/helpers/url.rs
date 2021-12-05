use linkify::LinkFinder;

use once_cell::sync::Lazy;

static LINK_FINDER: Lazy<LinkFinder> = Lazy::new(LinkFinder::new);

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

// Use `LinkFinder` to offload the raw link searching in plaintext
pub(crate) fn find_links(input: &str) -> impl Iterator<Item = linkify::Link> {
    LINK_FINDER.links(input)
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
            remove_get_params_and_fragment("https://example.org/index.html?foo=bar"),
            "https://example.org/index.html"
        );
        assert_eq!(
            remove_get_params_and_fragment("test.png?foo=bar"),
            "test.png"
        );

        assert_eq!(
            remove_get_params_and_fragment("https://example.org/index.html#anchor"),
            "https://example.org/index.html"
        );
        assert_eq!(
            remove_get_params_and_fragment("https://example.org/index.html?foo=bar#anchor"),
            "https://example.org/index.html"
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
