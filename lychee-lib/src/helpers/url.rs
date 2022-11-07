use linkify::LinkFinder;

use crate::{ErrorKind, Result};
use once_cell::sync::Lazy;
use reqwest::Url;

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

/// Get the base from a URL.
///
/// This removes the path and query parameters from a URL.
///
/// # Examples
///
/// ```rust
/// use lychee_lib::helpers::url::get_base;
///
/// let base = get_base("https://example.com/path/to/file.html?query=param#fragment");
/// assert_eq!(base, "https://example.com");
/// ```
pub fn base_url(url: &Url) -> Result<Url> {
    let mut url = url.clone();
    match url.path_segments_mut() {
        Ok(mut path) => {
            path.clear();
        }
        Err(_e) => {
            return Err(ErrorKind::ParseUrl(
                url::ParseError::RelativeUrlWithoutBase,
                "Unable to clear path segments".to_string(),
            )
            .into());
        }
    }

    url.set_query(None);

    Ok(url.clone())
}

/// Get the sitemap URL from a URL.
/// This is the URL of the sitemap.xml file.
///
/// By convention, the sitemap is located at the root of the domain.
///
/// # Examples
///
/// ```rust
/// use lychee_lib::helpers::url::get_sitemap_url;
///
/// let sitemap_url = get_sitemap_url("https://example.com/path/to/file.html?query=param#fragment");
/// assert_eq!(sitemap_url, "https://example.com/sitemap.xml");
/// ```
///
/// ```rust
/// use lychee_lib::helpers::url::get_sitemap_url;
///
/// let sitemap_url = get_sitemap_url("https://example.com/sitemap.xml");
/// assert_eq!(sitemap_url, "https://example.com/sitemap.xml");
/// ```
///
/// ```rust
/// use lychee_lib::helpers::url::get_sitemap_url;
///
/// let sitemap_url = get_sitemap_url("https://example.com/sitemap.xml?query=param#fragment");
/// assert_eq!(sitemap_url, "https://example.com/sitemap.xml");
/// ```
pub fn sitemap_url(url: &Url) -> Result<Url> {
    let base = base_url(&url)?;
    base.join("sitemap.xml")
        .map_err(|e| ErrorKind::ParseUrl(e, "Unable to join sitemap URL".to_string()))
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
