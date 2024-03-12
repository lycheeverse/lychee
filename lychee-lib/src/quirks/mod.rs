use crate::chain::Chainable;
use header::HeaderValue;
use http::header;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::{Request, Url};
use std::collections::HashMap;

static CRATES_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(https?://)?(www\.)?crates.io").unwrap());
static YOUTUBE_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(https?://)?(www\.)?(youtube\.com)").unwrap());
static YOUTUBE_SHORT_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(https?://)?(www\.)?(youtu\.?be)").unwrap());

// Retrieve a map of query params for the given request
fn query(request: &Request) -> HashMap<String, String> {
    request.url().query_pairs().into_owned().collect()
}

#[derive(Debug, Clone)]
pub(crate) struct Quirk {
    pub(crate) pattern: &'static Lazy<Regex>,
    pub(crate) rewrite: fn(Request) -> Request,
}

#[derive(Debug, Clone)]
pub(crate) struct Quirks {
    quirks: Vec<Quirk>,
}

impl Default for Quirks {
    fn default() -> Self {
        let quirks = vec![
            Quirk {
                pattern: &CRATES_PATTERN,
                rewrite: |mut request| {
                    request
                        .headers_mut()
                        .insert(header::ACCEPT, HeaderValue::from_static("text/html"));
                    request
                },
            },
            Quirk {
                pattern: &YOUTUBE_PATTERN,
                rewrite: |mut request| {
                    if request.url().path() != "/watch" {
                        return request;
                    }
                    if let Some(id) = query(&request).get("v") {
                        *request.url_mut() =
                            Url::parse(&format!("https://img.youtube.com/vi/{id}/0.jpg")).unwrap();
                    }
                    request
                },
            },
            Quirk {
                pattern: &YOUTUBE_SHORT_PATTERN,
                rewrite: |mut request| {
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
        ];
        Self { quirks }
    }
}

impl Chainable<Request> for Quirks {
    /// Handle quirks in a given request. Only the first quirk regex pattern
    /// matching the URL will be applied. The rest will be discarded for
    /// simplicity reasons. This limitation might be lifted in the future.
    fn handle(&mut self, request: Request) -> Request {
        for quirk in &self.quirks {
            if quirk.pattern.is_match(request.url().as_str()) {
                return (quirk.rewrite)(request);
            }
        }
        // Request was not modified
        request
    }
}

#[cfg(test)]
mod tests {
    use header::HeaderValue;
    use http::{header, Method};
    use reqwest::{Request, Url};

    use crate::chain::Chainable;

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
        let modified = Quirks::default().handle(request);

        assert_eq!(
            modified.headers().get(header::ACCEPT).unwrap(),
            HeaderValue::from_static("text/html")
        );
    }

    #[test]
    fn test_youtube_video_request() {
        let url = Url::parse("https://www.youtube.com/watch?v=NlKuICiT470&list=PLbWDhxwM_45mPVToqaIZNbZeIzFchsKKQ&index=7").unwrap();
        let request = Request::new(Method::GET, url);
        let modified = Quirks::default().handle(request);
        let expected_url = Url::parse("https://img.youtube.com/vi/NlKuICiT470/0.jpg").unwrap();

        assert_eq!(
            MockRequest(modified),
            MockRequest::new(Method::GET, expected_url)
        );
    }

    #[test]
    fn test_youtube_video_shortlink_request() {
        let url = Url::parse("https://youtu.be/Rvu7N4wyFpk?t=42").unwrap();
        let request = Request::new(Method::GET, url);
        let modified = Quirks::default().handle(request);
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
        let modified = Quirks::default().handle(request);

        assert_eq!(MockRequest(modified), MockRequest::new(Method::GET, url));
    }

    #[test]
    fn test_no_quirk_applied() {
        let url = Url::parse("https://endler.dev").unwrap();
        let request = Request::new(Method::GET, url.clone());
        let modified = Quirks::default().handle(request);

        assert_eq!(MockRequest(modified), MockRequest::new(Method::GET, url));
    }
}
