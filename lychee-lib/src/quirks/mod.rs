use crate::{
    Status,
    chain::{ChainResult, Handler},
};
use async_trait::async_trait;
use header::HeaderValue;
use http::header;
use log::debug;
use regex::{Captures, Regex};
use reqwest::{Request, Url};
use std::{collections::HashMap, sync::LazyLock};

static CRATES_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(https?://)?(www\.)?crates.io").unwrap());
static YOUTUBE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(https?://)?(www\.)?youtube(-nocookie)?\.com").unwrap());
static YOUTUBE_SHORT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(https?://)?(www\.)?(youtu\.be)").unwrap());
static GITHUB_BLOB_MARKDOWN_FRAGMENT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^https://github\.com/(?<user>.*?)/(?<repo>.*?)/blob/(?<path>.*?)/(?<file>.*\.(md|markdown)#.*)$")
        .unwrap()
});
static GITHUB_BLOB_LINE_FRAGMENT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^https://github\.com/(?<user>.*?)/(?<repo>.*?)/blob/(?<path>.*?)#L\d+(?:-L?\d+)?$")
        .unwrap()
});

// Retrieve a map of query params for the given request
fn query(request: &Request) -> HashMap<String, String> {
    request.url().query_pairs().into_owned().collect()
}

#[derive(Debug, Clone)]
pub(crate) struct Quirk {
    pub(crate) name: &'static str,
    pub(crate) pattern: &'static LazyLock<Regex>,
    pub(crate) rewrite: fn(Request, Captures) -> Request,
}

#[derive(Debug, Clone)]
pub(crate) struct Quirks {
    quirks: Vec<Quirk>,
}

impl Default for Quirks {
    fn default() -> Self {
        let quirks = vec![
            Quirk {
                name: "add accept header for crates.io",
                pattern: &CRATES_PATTERN,
                rewrite: |mut request, _| {
                    request
                        .headers_mut()
                        .insert(header::ACCEPT, HeaderValue::from_static("text/html"));
                    request
                },
            },
            Quirk {
                name: "check YouTube IDs via thumbnail",
                pattern: &YOUTUBE_PATTERN,
                rewrite: |mut request, _| {
                    // Extract video id if it's a video page
                    let video_id = match request.url().path() {
                        "/watch" => query(&request).get("v").map(ToOwned::to_owned),
                        path if path.starts_with("/embed/") => {
                            path.strip_prefix("/embed/").map(ToOwned::to_owned)
                        }
                        _ => return request,
                    };

                    // Only rewrite to thumbnail if we got a video id
                    if let Some(id) = video_id {
                        *request.url_mut() =
                            Url::parse(&format!("https://img.youtube.com/vi/{id}/0.jpg")).unwrap();
                    }

                    request
                },
            },
            Quirk {
                name: "check YouTube IDs via thumbnail (short link)",
                pattern: &YOUTUBE_SHORT_PATTERN,
                rewrite: |mut request, _| {
                    // Short links use the path as video id
                    let id = request.url().path().trim_start_matches('/');
                    if id.is_empty() {
                        return request;
                    }
                    *request.url_mut() =
                        Url::parse(&format!("https://img.youtube.com/vi/{id}/0.jpg")).unwrap();
                    request
                },
            },
            Quirk {
                name: "delete line number fragments in GitHub links",
                pattern: &GITHUB_BLOB_LINE_FRAGMENT_PATTERN,
                rewrite: |mut request, _| {
                    request.url_mut().set_fragment(None);
                    request
                },
            },
            Quirk {
                name: "fetch raw GitHub Markdown files",
                pattern: &GITHUB_BLOB_MARKDOWN_FRAGMENT_PATTERN,
                rewrite: |mut request, captures| {
                    let mut raw_url = String::new();
                    captures.expand(
                        "https://raw.githubusercontent.com/$user/$repo/$path/$file",
                        &mut raw_url,
                    );
                    *request.url_mut() = Url::parse(&raw_url).unwrap();
                    request
                },
            },
        ];
        Self { quirks }
    }
}

impl Quirks {
    /// Apply quirks to the given request, if applicable.
    ///
    /// Quirks are applied in sequence. The URL produced by earlier quirks can
    /// be transformed by later quirks.
    pub(crate) fn apply(&self, mut request: Request) -> Request {
        for quirk in &self.quirks {
            if let Some(captures) = quirk.pattern.captures(request.url().clone().as_str()) {
                debug!("Applied quirk '{}' to {}", quirk.name, request.url());
                request = (quirk.rewrite)(request, captures);
            }
        }
        request
    }
}

#[async_trait]
impl Handler<Request, Status> for Quirks {
    async fn handle(&mut self, input: Request) -> ChainResult<Request, Status> {
        ChainResult::Next(self.apply(input))
    }
}

#[cfg(test)]
mod tests {
    use header::HeaderValue;
    use http::{Method, header};
    use reqwest::{Request, Url};
    use rstest::rstest;

    use super::GITHUB_BLOB_LINE_FRAGMENT_PATTERN;
    use super::Quirks;

    #[derive(Debug)]
    struct MockRequest(Request);

    impl MockRequest {
        fn new(method: Method, url: Url) -> Self {
            Self(Request::new(method, url))
        }
    }

    impl PartialEq for MockRequest {
        fn eq(&self, other: &Self) -> bool {
            self.0.url() == other.0.url() && self.0.method() == other.0.method()
        }
    }

    #[test]
    fn test_cratesio_request() {
        let url = Url::parse("https://crates.io/crates/lychee").unwrap();
        let request = Request::new(Method::GET, url);
        let modified = Quirks::default().apply(request);

        assert_eq!(
            modified.headers().get(header::ACCEPT).unwrap(),
            HeaderValue::from_static("text/html")
        );
    }

    #[test]
    fn test_youtube_video_request() {
        let url = Url::parse("https://www.youtube.com/watch?v=NlKuICiT470&list=PLbWDhxwM_45mPVToqaIZNbZeIzFchsKKQ&index=7").unwrap();
        let request = Request::new(Method::GET, url);
        let modified = Quirks::default().apply(request);
        let expected_url = Url::parse("https://img.youtube.com/vi/NlKuICiT470/0.jpg").unwrap();

        assert_eq!(
            MockRequest(modified),
            MockRequest::new(Method::GET, expected_url)
        );
    }

    #[test]
    fn test_youtube_video_nocookie_request() {
        let url = Url::parse("https://www.youtube-nocookie.com/embed/BIguvia6AvM").unwrap();
        let request = Request::new(Method::GET, url);
        let modified = Quirks::default().apply(request);
        let expected_url = Url::parse("https://img.youtube.com/vi/BIguvia6AvM/0.jpg").unwrap();

        assert_eq!(
            MockRequest(modified),
            MockRequest::new(Method::GET, expected_url)
        );
    }

    #[test]
    fn test_youtube_video_shortlink_request() {
        let url = Url::parse("https://youtu.be/Rvu7N4wyFpk?t=42").unwrap();
        let request = Request::new(Method::GET, url);
        let modified = Quirks::default().apply(request);
        let expected_url = Url::parse("https://img.youtube.com/vi/Rvu7N4wyFpk/0.jpg").unwrap();

        assert_eq!(
            MockRequest(modified),
            MockRequest::new(Method::GET, expected_url)
        );
    }

    #[test]
    fn test_non_video_youtube_url_untouched() {
        let url = Url::parse("https://www.youtube.com/channel/UCaYhcUwRBNscFNUKTjgPFiA").unwrap();
        let request = Request::new(Method::GET, url.clone());
        let modified = Quirks::default().apply(request);

        assert_eq!(MockRequest(modified), MockRequest::new(Method::GET, url));
    }

    #[test]
    fn test_github_blob_markdown_fragment_request() {
        let cases = [
            (
                "https://github.com/moby/docker-image-spec/blob/main/spec.md#terminology",
                "https://raw.githubusercontent.com/moby/docker-image-spec/main/spec.md#terminology",
            ),
            (
                "https://github.com/moby/docker-image-spec/blob/main/spec.markdown#terminology",
                "https://raw.githubusercontent.com/moby/docker-image-spec/main/spec.markdown#terminology",
            ),
            (
                "https://github.com/moby/docker-image-spec/blob/main/spec.md",
                "https://github.com/moby/docker-image-spec/blob/main/spec.md",
            ),
            (
                "https://github.com/lycheeverse/lychee/blob/master/.gitignore#section",
                "https://github.com/lycheeverse/lychee/blob/master/.gitignore#section",
            ),
            (
                "https://github.com/lycheeverse/lychee/blob/v0.15.0/README.md#features",
                "https://raw.githubusercontent.com/lycheeverse/lychee/v0.15.0/README.md#features",
            ),
            (
                // GITHUB_BLOB_LINE_FRAGMENT_PATTERN should have precedence over
                // GITHUB_BLOB_MARKDOWN_FRAGMENT_PATTERN for line-number fragments.
                "https://github.com/lycheeverse/lychee/blob/v0.15.0/README.md#L1",
                "https://github.com/lycheeverse/lychee/blob/v0.15.0/README.md",
            ),
        ];
        for (origin, expect) in &cases {
            let url = Url::parse(origin).unwrap();
            let request = Request::new(Method::GET, url);
            let modified = Quirks::default().apply(request);

            assert_eq!(
                MockRequest(modified),
                MockRequest::new(Method::GET, Url::parse(expect).unwrap())
            );
        }
    }

    #[rstest]
    // Standard single line
    #[case(
        "https://github.com/lycheeverse/lychee/blob/master/README.md#L10",
        true
    )]
    // Standard range with double 'L'
    #[case(
        "https://github.com/lycheeverse/lychee/blob/master/src/main.rs#L10-L20",
        true
    )]
    // Shorthand range (no second 'L')
    #[case(
        "https://github.com/lycheeverse/lychee/blob/master/src/lib.rs#L5-15",
        true
    )]
    // Deeply nested path
    #[case(
        "https://github.com/user/repo/blob/feat/branch/path/to/file.txt#L1",
        true
    )]
    // Should match: Markdown file with line number fragment
    #[case("https://github.com/user/repo/blob/master/README.md#L2", true)]
    // Should NOT match: Markdown fragment (handled by the other regex)
    #[case(
        "https://github.com/user/repo/blob/master/README.md#installation",
        false
    )]
    // Should NOT match: Raw blob without line numbers
    #[case("https://github.com/user/repo/blob/master/src/main.rs", false)]
    // Should NOT match: Normal website URL
    #[case("https://github.com/user/repo", false)]
    fn test_github_blob_line_fragment_regex(#[case] url: &str, #[case] expected: bool) {
        assert_eq!(
            GITHUB_BLOB_LINE_FRAGMENT_PATTERN.is_match(url),
            expected,
            "Github blob line regex had unexpected outcome for {url}"
        );
    }

    #[test]
    fn test_no_quirk_applied() {
        let url = Url::parse("https://endler.dev").unwrap();
        let request = Request::new(Method::GET, url.clone());
        let modified = Quirks::default().apply(request);

        assert_eq!(MockRequest(modified), MockRequest::new(Method::GET, url));
    }
}
