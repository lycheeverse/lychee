use percent_encoding::percent_decode_str;
use reqwest::Url;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use crate::{
    basic_auth::BasicAuthExtractor,
    types::{uri::raw::RawUri, InputSource},
    utils::{path, url},
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
    root_dir: Option<&PathBuf>,
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
    root_dir: Option<&PathBuf>,
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
            let root_path = root_dir.unwrap().to_string_lossy();
            let combined = format!("{}{}", root_path, text);
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
                let mut path = root_dir.unwrap().clone();
                path.push(&text[1..]);
                path
            } else {
                // If text is just a fragment, we need to append it to the source path
                if is_anchor(&text) {
                    return Url::from_file_path(&source_path)
                        .map(|url| Uri { url })
                        .map_err(|_| ErrorKind::InvalidUrlFromPath(source_path.clone()))
                        .map(|mut uri| {
                            uri.url.set_fragment(Some(&text[1..]));
                            uri
                        });
                }

                // Relative paths: resolve relative to source
                match path::resolve(
                    source_path,
                    &PathBuf::from(&text),
                    false, // don't ignore absolute local links since we handled that case already
                ) {
                    Ok(Some(resolved)) => resolved,
                    _ => return Err(ErrorKind::InvalidPathToUri(text)),
                }
            };

            Url::from_file_path(&target_path)
                .map(|url| Uri { url })
                .map_err(|_| ErrorKind::InvalidUrlFromPath(target_path))
        }
        InputSource::String(s) => Err(ErrorKind::UnsupportedUriType(s.clone())),
        InputSource::RemoteUrl(url) => {
            let base_url = Url::parse(url.as_str()).map_err(|e| {
                ErrorKind::ParseUrl(e, format!("Could not parse base URL: {}", url))
            })?;
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

/// Create a URI from a file path
///
/// # Errors
///
/// - If the link text is an anchor and the file name cannot be extracted from the file path.
/// - If the path cannot be resolved.
/// - If the resolved path cannot be converted to a URL.
fn create_uri_from_file_path(
    file_path: &Path,
    link_text: &str,
    ignore_absolute_local_links: bool,
) -> Result<Uri> {
    let target_path = if is_anchor(link_text) {
        // For anchors, we need to append the anchor to the file name.
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ErrorKind::InvalidFile(file_path.to_path_buf()))?;

        format!("{file_name}{link_text}")
    } else {
        link_text.to_string()
    };
    let Ok(constructed_url) =
        resolve_and_create_url(file_path, &target_path, ignore_absolute_local_links)
    else {
        return Err(ErrorKind::InvalidPathToUri(target_path));
    };
    Ok(Uri {
        url: constructed_url,
    })
}

/// Create a URL from a path
///
/// `src_path` is the path of the source file.
/// `dest_path` is the path being linked to.
/// The optional `base_uri` specifies the base URI to resolve the destination path against.
///
/// # Errors
///
/// - If the percent-decoded destination path cannot be decoded as UTF-8.
/// - The path cannot be resolved
/// - The resolved path cannot be converted to a URL.
fn resolve_and_create_url(
    src_path: &Path,
    dest_path: &str,
    ignore_absolute_local_links: bool,
) -> Result<Url> {
    let (dest_path, fragment) = url::remove_get_params_and_separate_fragment(dest_path);

    // Decode the destination path to avoid double-encoding
    let decoded_dest = percent_decode_str(dest_path).decode_utf8()?;

    let Ok(Some(resolved_path)) = path::resolve(
        src_path,
        &PathBuf::from(&*decoded_dest),
        ignore_absolute_local_links,
    ) else {
        return Err(ErrorKind::InvalidPathToUri(decoded_dest.to_string()));
    };

    let Ok(mut url) = Url::from_file_path(&resolved_path) else {
        return Err(ErrorKind::InvalidUrlFromPath(resolved_path.clone()));
    };

    url.set_fragment(fragment);
    Ok(url)
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
/// it will not be added to the `HashSet`.
pub(crate) fn create(
    uris: Vec<RawUri>,
    source: &InputSource,
    root_dir: Option<&PathBuf>,
    base: Option<&Base>,
    extractor: Option<&BasicAuthExtractor>,
) -> HashSet<Result<Request>> {
    let base = base.cloned().or_else(|| Base::from_source(source));
    uris.into_iter()
        .map(|raw_uri| create_request(&raw_uri, source, root_dir, base.as_ref(), extractor))
        .collect()
}

fn prepend_root_dir_if_absolute_local_link(text: &str, root_dir: Option<&PathBuf>) -> String {
    if text.starts_with('/') {
        if let Some(path) = root_dir {
            if let Some(path_str) = path.to_str() {
                return format!("{path_str}{text}");
            }
        }
    }
    text.to_string()
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
    fn test_create_uri_from_path() {
        let result =
            resolve_and_create_url(&PathBuf::from("/README.md"), "test+encoding", true).unwrap();
        assert_eq!(result.as_str(), "file:///test+encoding");
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
        let root_dir = PathBuf::from("/tmp/lychee");
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
        let root_dir = PathBuf::from("/tmp/lychee");
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
        let root_dir = PathBuf::from("/tmp/lychee");
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
        let root_dir = PathBuf::from("/tmp/lychee");
        let source = InputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![RawUri::from("../parent")];
        let requests = create(uris, &source, Some(&root_dir), None, None);

        assert_eq!(requests.len(), 1);
        // assert!(requests
        //     .iter()
        //     .any(|r| r.as_ref().unwrap().uri.url.as_str() == "file:///parent"));

        assert_eq!(
            requests
                .iter()
                .next()
                .unwrap()
                .as_ref()
                .unwrap()
                .uri
                .url
                .as_str(),
            "file:///parent",
        );
    }

    #[test]
    fn test_fragment_url_resolution_from_root_dir() {
        let root_dir = PathBuf::from("/tmp/lychee");
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
        let root_dir = PathBuf::from("/tmp/lychee");
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
        let root_dir = PathBuf::from("/tmp/lychee");
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
        let root_dir = PathBuf::from("/tmp/lychee");
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
        let root_dir = PathBuf::from("/tmp/lychee");
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
        let root_dir = PathBuf::from("/tmp/lychee");
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
        let root_dir = PathBuf::from("/tmp/lychee");
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
        let root_dir = PathBuf::from("/foo/bar");
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
        let root_dir = PathBuf::from("/tmp/lychee");
        let source = InputSource::String(String::new());

        let raw_uri = RawUri::from("relative.html");
        let uri = try_parse_into_uri(&raw_uri, &source, Some(&root_dir), None).unwrap();

        assert_eq!(uri.url.as_str(), "file:///tmp/lychee/relative.html");
    }

    #[test]
    fn test_parse_absolute_path_into_uri() {
        let root_dir = PathBuf::from("/tmp/lychee");
        let source = InputSource::String(String::new());

        let raw_uri = RawUri::from("absolute.html");
        let uri = try_parse_into_uri(&raw_uri, &source, Some(&root_dir), None).unwrap();

        assert_eq!(uri.url.as_str(), "file:///tmp/lychee/absolute.html");
    }

    #[test]
    fn test_prepend_with_absolute_local_link_and_root_dir() {
        let text = "/absolute/path";
        let root_dir = PathBuf::from("/root");
        let result = prepend_root_dir_if_absolute_local_link(text, Some(&root_dir));
        assert_eq!(result, "/root/absolute/path");
    }

    #[test]
    fn test_prepend_with_absolute_local_link_and_no_root_dir() {
        let text = "/absolute/path";
        let result = prepend_root_dir_if_absolute_local_link(text, None);
        assert_eq!(result, "/absolute/path");
    }

    #[test]
    fn test_prepend_with_relative_link_and_root_dir() {
        let text = "relative/path";
        let root_dir = PathBuf::from("/root");
        let result = prepend_root_dir_if_absolute_local_link(text, Some(&root_dir));
        assert_eq!(result, "relative/path");
    }

    #[test]
    fn test_prepend_with_relative_link_and_no_root_dir() {
        let text = "relative/path";
        let result = prepend_root_dir_if_absolute_local_link(text, None);
        assert_eq!(result, "relative/path");
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
        let root_dir = PathBuf::from("/tmp/lychee");
        let source = InputSource::FsPath(PathBuf::from("/some/page.html"));

        let raw_uri = RawUri::from("#fragment");
        let uri = try_parse_into_uri(&raw_uri, &source, Some(&root_dir), None).unwrap();

        assert_eq!(uri.url.as_str(), "file:///some/page.html#fragment");
    }
}
