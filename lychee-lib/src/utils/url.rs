use linkify::LinkFinder;

use once_cell::sync::Lazy;
use url::Url;

use crate::types::{FragmentDirective, FRAGMENT_DIRECTIVE_DELIMITER};

static LINK_FINDER: Lazy<LinkFinder> = Lazy::new(LinkFinder::new);

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
pub(crate) fn find_links(input: &str) -> impl Iterator<Item = linkify::Link> {
    LINK_FINDER.links(input)
}

/// Fragment Directive feature trait
/// we will use the extension trait pattern to extend the Url to support Text Fragment feature
pub(crate) trait UrlExt {
    /// Checks if the url has a fragment and if the fragment is has the fragment directive delimiter embedded
    fn has_fragment_directive(&self) -> bool;

    /// Constructs `FragmentDirective`, if the URL contains a fragment and has fragment directive delimiter
    fn fragment_directive(&self) -> Option<FragmentDirective>;
}

impl UrlExt for Url {
    /// Returns whether the URL has fragment directive or not
    ///
    /// **Note:** Fragment Directive is possible only for the URL that has a fragment
    fn has_fragment_directive(&self) -> bool {
        if let Some(fragment) = self.fragment() {
            return fragment.contains(FRAGMENT_DIRECTIVE_DELIMITER);
        }

        false
    }

    /// Return this URL's fragment directive, if any
    ///
    /// **Note:** A fragment directive is part of the URL's fragment following the `:~:` delimiter
    fn fragment_directive(&self) -> Option<FragmentDirective> {
        if self.has_fragment_directive() {
            FragmentDirective::from_url(self)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test_fs_tree {
    use super::*;

    #[test]
    fn test_fragment_directive_through_url() {
        let url = Url::parse(
            "https://example.com#:~:text=prefix-,start,end,-suffix&text=unknown_directive",
        );
        match url {
            Ok(url) => {
                eprintln!(
                    "fragment is: {:#?}, {:?}",
                    url.fragment(),
                    url.fragment_directive()
                );
            }
            Err(e) => eprintln!("{e}"),
        }
    }

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
