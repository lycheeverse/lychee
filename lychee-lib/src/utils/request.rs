use reqwest::Url;
use std::{collections::HashSet, path::PathBuf};

use crate::{
    basic_auth::BasicAuthExtractor,
    types::{uri::raw::RawUri, InputSource, RootDir},
    utils::path,
    Base, BasicAuthCredentials, ErrorKind, Request, Result, Uri,
};

/// Extract basic auth credentials for a given URL.
fn extract_credentials(
    extractor: Option<&BasicAuthExtractor>,
    uri: &Uri,
) -> Option<BasicAuthCredentials> {
    extractor.as_ref().and_then(|ext| ext.matches(uri))
}

/// Create a request from a raw URI.
fn create_request(
    raw_uri: &RawUri,
    source: &InputSource,
    root_dir: Option<&RootDir>,
    base: Option<&Base>,
    extractor: Option<&BasicAuthExtractor>,
) -> Result<Request> {
    let uri = try_parse_into_uri(raw_uri, source, root_dir, base)?;
    let source = truncate_source(source);
    let element = raw_uri.element.clone();
    let attribute = raw_uri.attribute.clone();
    let credentials = extract_credentials(extractor, &uri);

    Ok(Request::new(uri, source, element, attribute, credentials))
}

/// Try to parse the raw URI into a `Uri`.
///
/// If the raw URI is not a valid URI, create a URI by joining the base URL with the text.
/// If the base URL is not available, create a URI from the file path.
///
/// # Errors
///
/// - If the text (the unparsed URI represented as a `String`) cannot be joined with the base
///   to create a valid URI.
/// - If a URI cannot be created from the file path.
/// - If the source is not a file path (i.e. the URI type is not supported).
fn try_parse_into_uri(
    raw_uri: &RawUri,
    source: &InputSource,
    root_dir: Option<&RootDir>,
    base: Option<&Base>,
) -> Result<Uri> {
    // First try direct URI parsing (handles explicit URLs)
    if let Ok(uri) = Uri::try_from(raw_uri.clone()) {
        return Ok(uri);
    }

    let text = raw_uri.text.clone();

    // Base URL takes precedence - all paths become URLs
    if let Some(base_url) = base {
        // For absolute paths with root_dir, insert root_dir after base
        if text.starts_with('/') && root_dir.is_some() {
            let root_dir = root_dir.unwrap();
            let combined = format!("{root_dir}{text}");
            return base_url
                .join(&combined)
                .map(|url| Uri { url })
                .ok_or_else(|| ErrorKind::InvalidBaseJoin(combined));
        }
        // Otherwise directly join with base
        return base_url
            .join(&text)
            .map(|url| Uri { url })
            .ok_or_else(|| ErrorKind::InvalidBaseJoin(text));
    }

    // No base URL - handle as filesystem paths
    match source {
        InputSource::FsPath(source_path) => {
            let target_path = if text.starts_with('/') && root_dir.is_some() {
                // Absolute paths: resolve via root_dir
                let root = root_dir.unwrap();
                root.join(&text[1..])
            } else {
                // If text is just a fragment, we need to append it to the source path
                if is_anchor(&text) {
                    return Url::from_file_path(source_path)
                        .map(|url| Uri { url })
                        .map_err(|()| ErrorKind::InvalidUrlFromPath(source_path.clone()))
                        .map(|mut uri| {
                            uri.url.set_fragment(Some(&text[1..]));
                            uri
                        });
                }

                // If source_path is relative and we have a root_dir,
                // we need to resolve both source_path and text relative to root_dir
                let resolved_source = if source_path.is_absolute() {
                    source_path.clone()
                } else {
                    match root_dir {
                        Some(dir) => dir.join(source_path),
                        None => source_path.clone(),
                    }
                };

                match path::resolve(&resolved_source, &PathBuf::from(&text), false) {
                    Ok(Some(resolved)) => resolved,
                    _ => return Err(ErrorKind::InvalidPathToUri(text)),
                }
            };

            Url::from_file_path(&target_path)
                .map(|url| Uri { url })
                .map_err(|()| ErrorKind::InvalidUrlFromPath(target_path))
        }
        InputSource::String(s) => {
            // If we have a root_dir, we can still resolve paths against it
            // even for string sources
            if let Some(root) = root_dir {
                let target_path = root.join(&text);
                return Url::from_file_path(&target_path)
                    .map(|url| Uri { url })
                    .map_err(|()| ErrorKind::InvalidUrlFromPath(target_path));
            }
            // Otherwise, we can't resolve the path
            Err(ErrorKind::UnsupportedUriType(s.clone()))
        }
        InputSource::RemoteUrl(url) => {
            let base_url = Url::parse(url.as_str())
                .map_err(|e| ErrorKind::ParseUrl(e, format!("Could not parse base URL: {url}")))?;
            base_url
                .join(&text)
                .map(|url| Uri { url })
                .map_err(|_| ErrorKind::InvalidBaseJoin(text))
        }
        _ => Err(ErrorKind::UnsupportedUriType(text)),
    }
}

// Taken from https://github.com/getzola/zola/blob/master/components/link_checker/src/lib.rs
pub(crate) fn is_anchor(text: &str) -> bool {
    text.starts_with('#')
}

/// Truncate the source in case it gets too long
///
/// This is only needed for string inputs.
/// For other inputs, the source is simply a "label" (an enum variant).
// TODO: This would not be necessary if we used `Cow` for the source.
fn truncate_source(source: &InputSource) -> InputSource {
    const MAX_TRUNCATED_STR_LEN: usize = 100;

    match source {
        InputSource::String(s) => {
            InputSource::String(s.chars().take(MAX_TRUNCATED_STR_LEN).collect())
        }
        other => other.clone(),
    }
}

/// Create requests out of the collected URLs.
/// Only keeps "valid" URLs. This filters out anchors for example.
///
/// If a URLs is ignored (because of the current settings),
/// it will not be added to the `Vector`.
pub(crate) fn create(
    uris: Vec<RawUri>,
    source: &InputSource,
    root_dir: Option<&RootDir>,
    base: Option<&Base>,
    extractor: Option<&BasicAuthExtractor>,
) -> HashSet<Result<Request>> {
    let base = base.cloned().or_else(|| Base::from_source(source));
    uris.into_iter()
        .map(|raw_uri| create_request(&raw_uri, source, root_dir, base.as_ref(), extractor))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_anchor() {
        assert!(is_anchor("#anchor"));
        assert!(!is_anchor("notan#anchor"));
    }

    #[test]
    fn test_relative_url_resolution() {
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = InputSource::String(String::new());

        let uris = vec![RawUri::from("relative.html")];
        let requests = create(uris, &source, None, Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(requests
            .iter()
            .any(|r| r.as_ref().unwrap().uri.url.as_str()
                == "https://example.com/path/relative.html"));
    }

    #[test]
    fn test_absolute_url_resolution() {
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = InputSource::String(String::new());

        let uris = vec![RawUri::from("https://another.com/page")];
        let requests = create(uris, &source, None, Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(requests
            .iter()
            .any(|r| r.as_ref().unwrap().uri.url.as_str() == "https://another.com/page"));
    }

    #[test]
    fn test_root_relative_url_resolution() {
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = InputSource::String(String::new());

        let uris = vec![RawUri::from("/root-relative")];
        let requests = create(uris, &source, None, Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(requests
            .iter()
            .any(|r| r.as_ref().unwrap().uri.url.as_str() == "https://example.com/root-relative"));
    }

    #[test]
    fn test_parent_directory_url_resolution() {
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = InputSource::String(String::new());

        let uris = vec![RawUri::from("../parent")];
        let requests = create(uris, &source, None, Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(requests
            .iter()
            .any(|r| r.as_ref().unwrap().uri.url.as_str() == "https://example.com/parent"));
    }

    #[test]
    fn test_fragment_url_resolution() {
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = InputSource::String(String::new());

        let uris = vec![RawUri::from("#fragment")];
        let requests = create(uris, &source, None, Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(requests.iter().any(|r| r.as_ref().unwrap().uri.url.as_str()
            == "https://example.com/path/page.html#fragment"));
    }

    #[test]
    fn test_relative_url_resolution_from_root_dir() {
        let root_dir = RootDir::new("/tmp/lychee").unwrap();
        let source = InputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![RawUri::from("relative.html")];
        let requests = create(uris, &source, Some(&root_dir), None, None);

        assert_eq!(requests.len(), 1);
        assert!(requests
            .iter()
            .any(|r| r.as_ref().unwrap().uri.url.as_str() == "file:///some/relative.html"));
    }

    #[test]
    fn test_absolute_url_resolution_from_root_dir() {
        let root_dir = RootDir::new("/tmp/lychee").unwrap();
        let source = InputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![RawUri::from("https://another.com/page")];
        let requests = create(uris, &source, Some(&root_dir), None, None);

        assert_eq!(requests.len(), 1);
        assert!(requests
            .iter()
            .any(|r| r.as_ref().unwrap().uri.url.as_str() == "https://another.com/page"));
    }

    #[test]
    fn test_root_relative_url_resolution_from_root_dir() {
        let root_dir = RootDir::new("/tmp/lychee").unwrap();
        let source = InputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![RawUri::from("/root-relative")];
        let requests = create(uris, &source, Some(&root_dir), None, None);

        assert_eq!(requests.len(), 1);
        assert!(requests
            .iter()
            .any(|r| r.as_ref().unwrap().uri.url.as_str() == "file:///tmp/lychee/root-relative"));
    }

    #[test]
    fn test_parent_directory_url_resolution_from_root_dir() {
        let root_dir = RootDir::new("/tmp/lychee").unwrap();
        let source = InputSource::FsPath(PathBuf::from("/some/page.html"));
        let uris = vec![RawUri::from("../parent")];
        let requests = create(uris, &source, Some(&root_dir), None, None);

        let url = requests
            .iter()
            .next()
            .unwrap()
            .as_ref()
            .unwrap()
            .uri
            .url
            .clone();
        assert_eq!(url.as_str(), "file:///parent");
    }

    #[test]
    fn test_fragment_url_resolution_from_root_dir() {
        let root_dir = RootDir::new("/tmp/lychee").unwrap();
        let source = InputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![RawUri::from("#fragment")];
        let requests = create(uris, &source, Some(&root_dir), None, None);

        assert_eq!(requests.len(), 1);
        assert!(requests
            .iter()
            .any(|r| r.as_ref().unwrap().uri.url.as_str() == "file:///some/page.html#fragment"));
    }

    #[test]
    fn test_relative_url_resolution_from_root_dir_and_base_url() {
        let root_dir = RootDir::new("/tmp/lychee").unwrap();
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = InputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![RawUri::from("relative.html")];
        let requests = create(uris, &source, Some(&root_dir), Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(requests
            .iter()
            .any(|r| r.as_ref().unwrap().uri.url.as_str()
                == "https://example.com/path/relative.html"));
    }

    #[test]
    fn test_absolute_url_resolution_from_root_dir_and_base_url() {
        let root_dir = RootDir::new("/tmp/lychee").unwrap();
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = InputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![RawUri::from("https://another.com/page")];
        let requests = create(uris, &source, Some(&root_dir), Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(requests
            .iter()
            .any(|r| r.as_ref().unwrap().uri.url.as_str() == "https://another.com/page"));
    }

    #[test]
    fn test_root_relative_url_resolution_from_root_dir_and_base_url() {
        let root_dir = RootDir::new("/tmp/lychee").unwrap();
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = InputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![RawUri::from("/root-relative")];
        let requests = create(uris, &source, Some(&root_dir), Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(requests.iter().any(|r| r.as_ref().unwrap().uri.url.as_str()
            == "https://example.com/tmp/lychee/root-relative"));
    }

    #[test]
    fn test_parent_directory_url_resolution_from_root_dir_and_base_url() {
        let root_dir = RootDir::new("/tmp/lychee").unwrap();
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = InputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![RawUri::from("../parent")];
        let requests = create(uris, &source, Some(&root_dir), Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(requests
            .iter()
            .any(|r| r.as_ref().unwrap().uri.url.as_str() == "https://example.com/parent"));
    }

    #[test]
    fn test_fragment_url_resolution_from_root_dir_and_base_url() {
        let root_dir = RootDir::new("/tmp/lychee").unwrap();
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = InputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![RawUri::from("#fragment")];
        let requests = create(uris, &source, Some(&root_dir), Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(requests.iter().any(|r| r.as_ref().unwrap().uri.url.as_str()
            == "https://example.com/path/page.html#fragment"));
    }

    #[test]
    fn test_no_base_url_resolution() {
        let source = InputSource::String(String::new());

        let uris = vec![RawUri::from("https://example.com/page")];
        let requests = create(uris, &source, None, None, None);

        assert_eq!(requests.len(), 1);
        assert!(requests
            .iter()
            .any(|r| r.as_ref().unwrap().uri.url.as_str() == "https://example.com/page"));
    }

    #[test]
    fn test_create_request_from_relative_file_path() {
        let root_dir = RootDir::new("/tmp/lychee").unwrap();
        let input_source = InputSource::FsPath(PathBuf::from("page.html"));

        let actual = create_request(
            &RawUri::from("file.html"),
            &input_source,
            Some(&root_dir),
            None,
            None,
        )
        .unwrap();

        assert_eq!(
            actual,
            Request::new(
                Uri {
                    url: Url::from_file_path("/tmp/lychee/file.html").unwrap()
                },
                input_source,
                None,
                None,
                None,
            )
        );
    }

    #[test]
    fn test_create_request_from_absolute_file_path() {
        let root_dir = RootDir::new("/foo/bar").unwrap();
        let input_source = InputSource::FsPath(PathBuf::from("/foo/bar/page.html"));

        // Use an absolute path that's outside the root directory
        let actual = create_request(
            &RawUri::from("/baz/example.html"),
            &input_source,
            Some(&root_dir),
            None,
            None,
        )
        .unwrap();

        assert_eq!(
            actual,
            Request::new(
                Uri {
                    url: Url::from_file_path("/foo/bar/baz/example.html").unwrap()
                },
                input_source,
                None,
                None,
                None,
            )
        );
    }

    #[test]
    fn test_parse_relative_path_into_uri() {
        let root_dir = RootDir::new("/tmp/lychee").unwrap();
        let source = InputSource::String(String::new());

        let raw_uri = RawUri::from("relative.html");
        let uri = try_parse_into_uri(&raw_uri, &source, Some(&root_dir), None).unwrap();

        assert_eq!(uri.url.as_str(), "file:///tmp/lychee/relative.html");
    }

    #[test]
    fn test_parse_absolute_path_into_uri() {
        let root_dir = RootDir::new("/tmp/lychee").unwrap();
        let source = InputSource::String(String::new());

        let raw_uri = RawUri::from("absolute.html");
        let uri = try_parse_into_uri(&raw_uri, &source, Some(&root_dir), None).unwrap();

        assert_eq!(uri.url.as_str(), "file:///tmp/lychee/absolute.html");
    }

    #[test]
    fn test_parse_url_with_anchor() {
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = InputSource::String(String::new());

        let raw_uri = RawUri::from("#fragment");
        let uri = try_parse_into_uri(&raw_uri, &source, None, Some(&base)).unwrap();

        assert_eq!(
            uri.url.as_str(),
            "https://example.com/path/page.html#fragment"
        );
    }

    #[test]
    fn test_parse_url_to_different_page_with_anchor() {
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = InputSource::String(String::new());

        let raw_uri = RawUri::from("other-page.html#fragment");
        let uri = try_parse_into_uri(&raw_uri, &source, None, Some(&base)).unwrap();

        assert_eq!(
            uri.url.as_str(),
            "https://example.com/path/other-page.html#fragment"
        );
    }

    #[test]
    fn test_parse_url_from_path_with_anchor() {
        let root_dir = RootDir::new("/tmp/lychee").unwrap();
        let source = InputSource::FsPath(PathBuf::from("/some/page.html"));

        let raw_uri = RawUri::from("#fragment");
        let uri = try_parse_into_uri(&raw_uri, &source, Some(&root_dir), None).unwrap();

        assert_eq!(uri.url.as_str(), "file:///some/page.html#fragment");
    }
}
