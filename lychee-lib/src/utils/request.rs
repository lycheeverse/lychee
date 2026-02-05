use percent_encoding::percent_decode_str;
use reqwest::Url;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::{
    Base, BasicAuthCredentials, ErrorKind, LycheeResult, Request, RequestError, Uri,
    basic_auth::BasicAuthExtractor,
    types::{ResolvedInputSource, uri::raw::RawUri},
    utils::{path, url},
};

/// Extract basic auth credentials for a given URL.
pub(crate) fn extract_credentials(
    extractor: Option<&BasicAuthExtractor>,
    uri: &Uri,
) -> Option<BasicAuthCredentials> {
    extractor.as_ref().and_then(|ext| ext.matches(uri))
}

/// Create a request from a raw URI.
fn create_request(
    raw_uri: &RawUri,
    source: &ResolvedInputSource,
    root_dir: Option<&PathBuf>,
    base: Option<&Base>,
    extractor: Option<&BasicAuthExtractor>,
) -> LycheeResult<Request> {
    let uri = try_parse_into_uri(raw_uri, source, root_dir, base)?;
    let source = source.clone();
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
    source: &ResolvedInputSource,
    root_dir: Option<&PathBuf>,
    base: Option<&Base>,
) -> LycheeResult<Uri> {
    let text = prepend_root_dir_if_absolute_local_link(&raw_uri.text, root_dir);
    let uri = match Uri::try_from(raw_uri.clone()) {
        Ok(uri) => uri,
        Err(_) => match base {
            Some(base_url) => match base_url.join(&text) {
                Some(url) => Uri { url },
                None => return Err(ErrorKind::InvalidBaseJoin(text.clone())),
            },
            None => match source {
                ResolvedInputSource::FsPath(root) => {
                    // Absolute local links (i.e. links starting with '/') are
                    // only supported if a `root_dir` is provided, otherwise
                    // they are ignored. This is because without a `root_dir`,
                    // we cannot determine the absolute path to resolve the link
                    // to.
                    let ignore_absolute_local_links = root_dir.is_none();
                    create_uri_from_file_path(root, &text, ignore_absolute_local_links)?
                }
                _ => return Err(ErrorKind::UnsupportedUriType(text)),
            },
        },
    };
    Ok(uri)
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
) -> LycheeResult<Uri> {
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

/// Create requests out of the collected URLs.
/// Returns a vector of valid URLs and errors. Valid URLs are deduplicated,
/// request errors are not deduplicated.
///
/// If a URLs is ignored (because of the current settings),
/// it will not be added to the results.
pub(crate) fn create(
    uris: Vec<RawUri>,
    source: &ResolvedInputSource,
    root_dir: Option<&PathBuf>,
    base: Option<&Base>,
    extractor: Option<&BasicAuthExtractor>,
) -> Vec<Result<Request, RequestError>> {
    let base = base.cloned().or_else(|| Base::from_source(source));

    let mut requests = HashSet::<Request>::new();
    let mut errors = Vec::<RequestError>::new();

    for raw_uri in uris {
        let result = create_request(&raw_uri, source, root_dir, base.as_ref(), extractor);
        match result {
            Ok(request) => {
                requests.insert(request);
            }
            Err(e) => errors.push(RequestError::CreateRequestItem(
                raw_uri.clone(),
                source.clone(),
                e,
            )),
        }
    }

    (requests.into_iter().map(Result::Ok))
        .chain(errors.into_iter().map(Result::Err))
        .collect()
}

/// Create a URI from a path
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
) -> LycheeResult<Url> {
    let (dest_path, fragment) = url::remove_get_params_and_separate_fragment(dest_path);

    // Decode the destination path to avoid double-encoding
    // This addresses the issue mentioned in the original comment about double-encoding
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

fn prepend_root_dir_if_absolute_local_link(text: &str, root_dir: Option<&PathBuf>) -> String {
    if text.starts_with('/')
        && let Some(path) = root_dir
        && let Some(path_str) = path.to_str()
    {
        return format!("{path_str}{text}");
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::num::NonZeroUsize;

    use crate::types::uri::raw::RawUriSpan;

    use super::*;

    /// Create requests from the given raw URIs and returns requests that were
    /// constructed successfully, silently ignoring link parsing errors.
    ///
    /// This reduces the `Result` handling which is needed in test cases. Test
    /// cases can still detect the unexpected appearance of errors by the
    /// length being different.
    fn create_ok_only(
        uris: Vec<RawUri>,
        source: &ResolvedInputSource,
        root_dir: Option<&PathBuf>,
        base: Option<&Base>,
        extractor: Option<&BasicAuthExtractor>,
    ) -> Vec<Request> {
        create(uris, source, root_dir, base, extractor)
            .into_iter()
            .filter_map(Result::ok)
            .collect()
    }

    fn raw_uri(text: &'static str) -> RawUri {
        RawUri {
            text: text.to_string(),
            element: None,
            attribute: None,
            span: RawUriSpan {
                line: NonZeroUsize::MAX,
                column: None,
            },
        }
    }

    #[test]
    fn test_is_anchor() {
        assert!(is_anchor("#anchor"));
        assert!(!is_anchor("notan#anchor"));
    }

    #[test]
    fn test_create_uri_from_path() {
        #[cfg(not(windows))]
        let src = PathBuf::from("/README.md");
        #[cfg(windows)]
        let src = PathBuf::from("C:\\README.md");

        let result = resolve_and_create_url(&src, "test+encoding", true).unwrap();

        #[cfg(not(windows))]
        let expected = "file:///test+encoding";
        #[cfg(windows)]
        let expected = "file:///C:/test+encoding";

        assert_eq!(result.as_str(), expected);
    }

    #[test]
    fn test_relative_url_resolution() {
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::String(Cow::Borrowed(""));

        let uris = vec![raw_uri("relative.html")];
        let requests = create_ok_only(uris, &source, None, Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://example.com/path/relative.html")
        );
    }

    #[test]
    fn test_absolute_url_resolution() {
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::String(Cow::Borrowed(""));

        let uris = vec![raw_uri("https://another.com/page")];
        let requests = create_ok_only(uris, &source, None, Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://another.com/page")
        );
    }

    #[test]
    fn test_root_relative_url_resolution() {
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::String(Cow::Borrowed(""));

        let uris = vec![raw_uri("/root-relative")];
        let requests = create_ok_only(uris, &source, None, Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://example.com/root-relative")
        );
    }

    #[test]
    fn test_parent_directory_url_resolution() {
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::String(Cow::Borrowed(""));

        let uris = vec![raw_uri("../parent")];
        let requests = create_ok_only(uris, &source, None, Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://example.com/parent")
        );
    }

    #[test]
    fn test_fragment_url_resolution() {
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::String(Cow::Borrowed(""));

        let uris = vec![raw_uri("#fragment")];
        let requests = create_ok_only(uris, &source, None, Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://example.com/path/page.html#fragment")
        );
    }

    #[test]
    fn test_relative_url_resolution_from_root_dir() {
        #[cfg(not(windows))]
        let root_dir = PathBuf::from("/tmp/lychee");
        #[cfg(windows)]
        let root_dir = PathBuf::from("C:\\tmp\\lychee");

        #[cfg(not(windows))]
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));
        #[cfg(windows)]
        let source = ResolvedInputSource::FsPath(PathBuf::from("C:\\some\\page.html"));

        let uris = vec![raw_uri("relative.html")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), None, None);

        assert_eq!(requests.len(), 1);

        #[cfg(not(windows))]
        let expected = "file:///some/relative.html";
        #[cfg(windows)]
        let expected = "file:///C:/some/relative.html";

        assert!(requests.iter().any(|r| r.uri.url.as_str() == expected));
    }

    #[test]
    fn test_absolute_url_resolution_from_root_dir() {
        let root_dir = PathBuf::from("/tmp/lychee");
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("https://another.com/page")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), None, None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://another.com/page")
        );
    }

    #[test]
    fn test_root_relative_url_resolution_from_root_dir() {
        #[cfg(not(windows))]
        let root_dir = PathBuf::from("/tmp/lychee");
        #[cfg(windows)]
        let root_dir = PathBuf::from("C:\\tmp\\lychee");

        #[cfg(not(windows))]
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));
        #[cfg(windows)]
        let source = ResolvedInputSource::FsPath(PathBuf::from("C:\\some\\page.html"));

        let uris = vec![raw_uri("/root-relative")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), None, None);

        assert_eq!(requests.len(), 1);

        #[cfg(not(windows))]
        let expected = "file:///tmp/lychee/root-relative";
        #[cfg(windows)]
        let expected = "file:///C:/tmp/lychee/root-relative";

        assert!(requests.iter().any(|r| r.uri.url.as_str() == expected));
    }

    #[test]
    fn test_parent_directory_url_resolution_from_root_dir() {
        #[cfg(not(windows))]
        let root_dir = PathBuf::from("/tmp/lychee");
        #[cfg(windows)]
        let root_dir = PathBuf::from("C:\\tmp\\lychee");

        #[cfg(not(windows))]
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));
        #[cfg(windows)]
        let source = ResolvedInputSource::FsPath(PathBuf::from("C:\\some\\page.html"));

        let uris = vec![raw_uri("../parent")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), None, None);

        assert_eq!(requests.len(), 1);

        #[cfg(not(windows))]
        let expected = "file:///parent";
        #[cfg(windows)]
        let expected = "file:///C:/parent";

        assert!(requests.iter().any(|r| r.uri.url.as_str() == expected));
    }

    #[test]
    fn test_fragment_url_resolution_from_root_dir() {
        #[cfg(not(windows))]
        let root_dir = PathBuf::from("/tmp/lychee");
        #[cfg(windows)]
        let root_dir = PathBuf::from("C:\\tmp\\lychee");

        #[cfg(not(windows))]
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));
        #[cfg(windows)]
        let source = ResolvedInputSource::FsPath(PathBuf::from("C:\\some\\page.html"));

        let uris = vec![raw_uri("#fragment")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), None, None);

        assert_eq!(requests.len(), 1);

        #[cfg(not(windows))]
        let expected = "file:///some/page.html#fragment";
        #[cfg(windows)]
        let expected = "file:///C:/some/page.html#fragment";

        assert!(requests.iter().any(|r| r.uri.url.as_str() == expected));
    }

    #[test]
    fn test_relative_url_resolution_from_root_dir_and_base_url() {
        let root_dir = PathBuf::from("/tmp/lychee");
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("relative.html")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://example.com/path/relative.html")
        );
    }

    #[test]
    fn test_absolute_url_resolution_from_root_dir_and_base_url() {
        let root_dir = PathBuf::from("/tmp/lychee");
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("https://another.com/page")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://another.com/page")
        );
    }

    #[test]
    fn test_root_relative_url_resolution_from_root_dir_and_base_url() {
        let root_dir = PathBuf::from("/tmp/lychee");
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("/root-relative")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://example.com/tmp/lychee/root-relative")
        );
    }

    #[test]
    fn test_parent_directory_url_resolution_from_root_dir_and_base_url() {
        let root_dir = PathBuf::from("/tmp/lychee");
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("../parent")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://example.com/parent")
        );
    }

    #[test]
    fn test_fragment_url_resolution_from_root_dir_and_base_url() {
        let root_dir = PathBuf::from("/tmp/lychee");
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("#fragment")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), Some(&base), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://example.com/path/page.html#fragment")
        );
    }

    #[test]
    fn test_no_base_url_resolution() {
        let source = ResolvedInputSource::String(Cow::Borrowed(""));

        let uris = vec![raw_uri("https://example.com/page")];
        let requests = create_ok_only(uris, &source, None, None, None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://example.com/page")
        );
    }

    #[test]
    fn test_create_request_from_relative_file_path() {
        #[cfg(not(windows))]
        let base_path = PathBuf::from("/tmp/lychee");
        #[cfg(windows)]
        let base_path = PathBuf::from("C:\\tmp\\lychee");

        let base = Base::Local(base_path);
        let input_source = ResolvedInputSource::FsPath(PathBuf::from("page.html"));

        let actual = create_request(
            &raw_uri("file.html"),
            &input_source,
            None,
            Some(&base),
            None,
        )
        .unwrap();

        #[cfg(not(windows))]
        let expected = "/tmp/lychee/file.html";
        #[cfg(windows)]
        let expected = "C:\\tmp\\lychee\\file.html";

        assert_eq!(
            actual,
            Request::new(
                Uri {
                    url: Url::from_file_path(expected).unwrap()
                },
                input_source,
                None,
                None,
                None,
            )
        );
    }

    #[test]
    fn test_create_request_from_relative_file_path_errors() {
        // relative links unsupported from stdin
        assert!(
            create_request(
                &raw_uri("file.html"),
                &ResolvedInputSource::Stdin,
                None,
                None,
                None,
            )
            .is_err()
        );

        // error because no root-dir and no base-url
        #[cfg(not(windows))]
        let file_path = "/file.html";
        #[cfg(windows)]
        let file_path = "C:\\file.html";

        assert!(
            create_request(
                &raw_uri(file_path),
                &ResolvedInputSource::FsPath(PathBuf::from("page.html")),
                None,
                None,
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn test_create_request_from_absolute_file_path() {
        #[cfg(not(windows))]
        let base_path = PathBuf::from("/tmp/lychee");
        #[cfg(windows)]
        let base_path = PathBuf::from("C:\\tmp\\lychee");

        #[cfg(not(windows))]
        let input_path = PathBuf::from("/tmp/lychee/page.html");
        #[cfg(windows)]
        let input_path = PathBuf::from("C:\\tmp\\lychee\\page.html");

        let base = Base::Local(base_path);
        let input_source = ResolvedInputSource::FsPath(input_path);

        #[cfg(not(windows))]
        let absolute_file = "/usr/local/share/doc/example.html";
        #[cfg(windows)]
        let absolute_file = "C:\\usr\\local\\share\\doc\\example.html";

        // Use an absolute path that's outside the base directory
        let actual = create_request(
            &raw_uri(absolute_file),
            &input_source,
            None,
            Some(&base),
            None,
        )
        .unwrap();

        assert_eq!(
            actual,
            Request::new(
                Uri {
                    url: Url::from_file_path(absolute_file).unwrap()
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
        #[cfg(not(windows))]
        let base_path = PathBuf::from("/tmp/lychee");
        #[cfg(windows)]
        let base_path = PathBuf::from("C:\\tmp\\lychee");

        let base = Base::Local(base_path);
        let source = ResolvedInputSource::String(Cow::Borrowed(""));

        let raw_uri = raw_uri("relative.html");
        let uri = try_parse_into_uri(&raw_uri, &source, None, Some(&base)).unwrap();

        #[cfg(not(windows))]
        let expected = "file:///tmp/lychee/relative.html";
        #[cfg(windows)]
        let expected = "file:///C:/tmp/lychee/relative.html";

        assert_eq!(uri.url.as_str(), expected);
    }

    #[test]
    fn test_parse_absolute_path_into_uri() {
        #[cfg(not(windows))]
        let base_path = PathBuf::from("/tmp/lychee");
        #[cfg(windows)]
        let base_path = PathBuf::from("C:\\tmp\\lychee");

        let base = Base::Local(base_path);
        let source = ResolvedInputSource::String(Cow::Borrowed(""));

        let raw_uri = raw_uri("absolute.html");
        let uri = try_parse_into_uri(&raw_uri, &source, None, Some(&base)).unwrap();

        #[cfg(not(windows))]
        let expected = "file:///tmp/lychee/absolute.html";
        #[cfg(windows)]
        let expected = "file:///C:/tmp/lychee/absolute.html";

        assert_eq!(uri.url.as_str(), expected);
    }

    #[test]
    fn test_prepend_with_absolute_local_link_and_root_dir() {
        let text = "/absolute/path";
        #[cfg(not(windows))]
        let root_dir = PathBuf::from("/root");
        #[cfg(windows)]
        let root_dir = PathBuf::from("C:\\root");

        let result = prepend_root_dir_if_absolute_local_link(text, Some(&root_dir));

        #[cfg(not(windows))]
        let expected = "/root/absolute/path";
        #[cfg(windows)]
        let expected = "C:\\root/absolute/path";

        assert_eq!(result, expected);
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
}
