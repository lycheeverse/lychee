use reqwest::Url;
use std::collections::HashSet;
use std::path::Path;

use crate::{
    BaseInfo, BasicAuthCredentials, LycheeResult, Request, RequestError, Uri,
    basic_auth::BasicAuthExtractor,
    types::{ResolvedInputSource, uri::raw::RawUri},
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
    root_dir: Option<&Path>,
    base: &BaseInfo,
    extractor: Option<&BasicAuthExtractor>,
) -> LycheeResult<Request> {
    let uri = try_parse_into_uri(raw_uri, root_dir, base)?;
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
    root_dir: Option<&Path>,
    base: &BaseInfo,
) -> LycheeResult<Uri> {
    // TODO: this conversion should be hoisted up the call stack
    let root_dir = root_dir.and_then(|x| Url::from_directory_path(x).ok());
    Ok(base
        .parse_url_text_with_root_dir(&raw_uri.text, root_dir.as_ref())?
        .into())
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
    root_dir: Option<&Path>,
    fallback_base: &BaseInfo,
    extractor: Option<&BasicAuthExtractor>,
) -> Vec<Result<Request, RequestError>> {
    let source_base = match source.to_url() {
        Ok(None) => BaseInfo::no_info(),
        Ok(Some(url)) => BaseInfo::from_source_url(&url),
        Err(e) => {
            // TODO: GetInputContent is not quite the right error.
            return vec![Err(RequestError::GetInputContent(source.clone().into(), e))];
        }
    };

    // TODO: avoid use_fs_root_as_origin once base-url sementics are clarified
    let fallback_base = fallback_base.use_fs_root_as_origin();
    let base = source_base.or_fallback(&fallback_base);

    let mut requests = HashSet::<Request>::new();
    let mut errors = Vec::<RequestError>::new();

    for raw_uri in uris {
        let result = create_request(&raw_uri, source, root_dir, base, extractor);
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

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::num::NonZeroUsize;
    use std::path::PathBuf;

    use crate::Request;
    use crate::types::uri::raw::{RawUri, RawUriSpan};

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
        root_dir: Option<&Path>,
        base: &BaseInfo,
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
    fn test_relative_url_resolution() {
        let base = BaseInfo::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::String(Cow::Borrowed(""));

        let uris = vec![raw_uri("relative.html")];
        let requests = create_ok_only(uris, &source, None, &base, None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://example.com/path/relative.html")
        );
    }

    #[test]
    fn test_absolute_url_resolution() {
        let base = BaseInfo::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::String(Cow::Borrowed(""));

        let uris = vec![raw_uri("https://another.com/page")];
        let requests = create_ok_only(uris, &source, None, &base, None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://another.com/page")
        );
    }

    #[test]
    fn test_root_relative_url_resolution() {
        let base = BaseInfo::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::String(Cow::Borrowed(""));

        let uris = vec![raw_uri("/root-relative")];
        let requests = create_ok_only(uris, &source, None, &base, None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://example.com/root-relative")
        );
    }

    #[test]
    fn test_parent_directory_url_resolution() {
        let base = BaseInfo::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::String(Cow::Borrowed(""));

        let uris = vec![raw_uri("../parent")];
        let requests = create_ok_only(uris, &source, None, &base, None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://example.com/parent")
        );
    }

    #[test]
    fn test_fragment_url_resolution() {
        let base = BaseInfo::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::String(Cow::Borrowed(""));

        let uris = vec![raw_uri("#fragment")];
        let requests = create_ok_only(uris, &source, None, &base, None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://example.com/path/page.html#fragment")
        );
    }

    #[test]
    fn test_relative_url_resolution_from_root_dir() {
        let root_dir = PathBuf::from("/tmp/lychee");
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("relative.html")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), &BaseInfo::default(), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "file:///some/relative.html")
        );
    }

    #[test]
    fn test_absolute_url_resolution_from_root_dir() {
        let root_dir = PathBuf::from("/tmp/lychee");
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("https://another.com/page")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), &BaseInfo::default(), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://another.com/page")
        );
    }

    #[test]
    fn test_root_relative_url_resolution_from_root_dir() {
        let root_dir = PathBuf::from("/tmp/lychee");
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("/root-relative")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), &BaseInfo::default(), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "file:///tmp/lychee/root-relative")
        );
    }

    #[test]
    fn test_parent_directory_url_resolution_from_root_dir() {
        let root_dir = PathBuf::from("/tmp/lychee");
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("../parent")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), &BaseInfo::default(), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "file:///parent")
        );
    }

    #[test]
    fn test_fragment_url_resolution_from_root_dir() {
        let root_dir = PathBuf::from("/tmp/lychee");
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("#fragment")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), &BaseInfo::no_info(), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "file:///some/page.html#fragment")
        );
    }

    #[test]
    fn test_relative_url_resolution_from_root_dir_and_base_url() {
        let root_dir = PathBuf::from("/tmp/lychee");
        let base = BaseInfo::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("relative.html")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), &base, None);

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
        let base = BaseInfo::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("https://another.com/page")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), &base, None);

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
        let base = BaseInfo::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("/root-relative")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), &base, None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://example.com/root-relative")
        );
    }

    #[test]
    fn test_parent_directory_url_resolution_from_root_dir_and_base_url() {
        let root_dir = PathBuf::from("/tmp/lychee");
        let base = BaseInfo::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("../parent")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), &base, None);

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
        let base = BaseInfo::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));

        let uris = vec![raw_uri("#fragment")];
        let requests = create_ok_only(uris, &source, Some(&root_dir), &base, None);

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
        let requests = create_ok_only(uris, &source, None, &BaseInfo::default(), None);

        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .any(|r| r.uri.url.as_str() == "https://example.com/page")
        );
    }

    #[test]
    fn test_create_request_from_relative_file_path() {
        let base = BaseInfo::from_path(&PathBuf::from("/tmp/lychee")).unwrap();
        let input_source = ResolvedInputSource::FsPath(PathBuf::from("page.html"));

        let actual =
            create_request(&raw_uri("file.html"), &input_source, None, &base, None).unwrap();

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
    fn test_create_request_from_relative_file_path_errors() {
        // relative links unsupported from stdin
        assert!(
            create_request(
                &raw_uri("file.html"),
                &ResolvedInputSource::Stdin,
                None,
                &BaseInfo::default(),
                None,
            )
            .is_err()
        );

        // error because no root-dir and no base-url
        assert!(
            create_request(
                &raw_uri("/file.html"),
                &ResolvedInputSource::FsPath(PathBuf::from("page.html")),
                None,
                &BaseInfo::no_info(),
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn test_create_request_from_absolute_file_path() {
        let base = BaseInfo::from_path(&PathBuf::from("/tmp/lychee")).unwrap();
        let input_source = ResolvedInputSource::FsPath(PathBuf::from("/tmp/lychee/page.html"));

        // Use an absolute path that's outside the base directory
        let actual = create_request(
            &raw_uri("/usr/local/share/doc/example.html"),
            &input_source,
            None,
            &base,
            None,
        )
        .unwrap();

        assert_eq!(
            actual,
            Request::new(
                Uri {
                    url: Url::from_file_path("/tmp/lychee/usr/local/share/doc/example.html")
                        .unwrap()
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
        let base = BaseInfo::from_path(&PathBuf::from("/tmp/lychee")).unwrap();

        let raw_uri = raw_uri("relative.html");
        let uri = try_parse_into_uri(&raw_uri, None, &base).unwrap();

        assert_eq!(uri.url.as_str(), "file:///tmp/lychee/relative.html");
    }

    #[test]
    fn test_parse_absolute_path_into_uri() {
        let base = BaseInfo::from_path(&PathBuf::from("/tmp/lychee")).unwrap();

        let raw_uri = raw_uri("absolute.html");
        let uri = try_parse_into_uri(&raw_uri, None, &base).unwrap();

        assert_eq!(uri.url.as_str(), "file:///tmp/lychee/absolute.html");
    }
}
