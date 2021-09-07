use linkify::LinkFinder;

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

/// Determine if an element's attribute contains a link / URL.
pub(crate) fn elem_attr_is_link(attr_name: &str, elem_name: &str) -> bool {
    // See a comprehensive list of attributes that might contain URLs/URIs
    // over at: https://developer.mozilla.org/en-US/docs/Web/HTML/Attributes
    matches!(
        (attr_name, elem_name),
        ("href" | "src" | "srcset" | "cite", _) | ("data", "object") | ("onhashchange", "body")
    )
}

// Taken from https://github.com/getzola/zola/blob/master/components/link_checker/src/lib.rs
pub(crate) fn is_anchor(url: &str) -> bool {
    url.starts_with('#')
}

// Use `LinkFinder` to offload the raw link searching in plaintext
pub(crate) fn find_links(input: &str) -> Vec<linkify::Link> {
    let finder = LinkFinder::new();
    finder.links(input).collect()
}

#[cfg(test)]
mod test_fs_tree {
    use super::*;

    #[test]
    fn test_is_anchor() {
        assert!(is_anchor("#anchor"));
        assert!(!is_anchor("notan#anchor"));
    }

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
