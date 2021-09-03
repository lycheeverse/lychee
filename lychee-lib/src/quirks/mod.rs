use header::HeaderValue;
use http::{header, Method};
use regex::Regex;
use reqwest::{Request, Url};
use std::collections::HashMap;

/// Sadly some pages only return plaintext results if Google is trying to crawl them.
const GOOGLEBOT: &str = "Mozilla/5.0 (compatible; Googlebot/2.1; +http://google.com/bot.html)";

// Retrieve a map of query params for the given request
fn query(request: &Request) -> HashMap<String, String> {
    request.url().query_pairs().into_owned().collect()
}

#[derive(Debug, Clone)]
pub(crate) struct Quirk {
    pub(crate) pattern: Regex,
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
                // Twitter cut off the ability to read a tweet by fetching its
                // URL with a normal HTTP GET. Only Googlebot will get a plain
                // HTML response.
                // See https://twitter.com/zarfeblong/status/1339742840142872577
                pattern: Regex::new(r"^(https?://)?(www\.)?twitter.com").unwrap(),
                rewrite: |request| {
                    let mut out = request;
                    *out.method_mut() = Method::HEAD;
                    out.headers_mut()
                        .insert(header::USER_AGENT, GOOGLEBOT.parse().unwrap());
                    out
                },
            },
            Quirk {
                pattern: Regex::new(r"^(https?://)?(www\.)?crates.io").unwrap(),
                rewrite: |request| {
                    let mut out = request;
                    out.headers_mut()
                        .insert(header::ACCEPT, HeaderValue::from_static("text/html"));
                    out
                },
            },
            Quirk {
                // Even missing YouTube videos return a 200, therefore we use
                // the thumbnail endpoint instead
                // (https://img.youtube.com/vi/{video_id}/0.jpg).
                // This works for all known video visibilities.
                // See https://github.com/lycheeverse/lychee/issues/214#issuecomment-819103393)
                pattern: Regex::new(r"^(https?://)?(www\.)?(youtube\.com|youtu\.?be)").unwrap(),
                rewrite: |request| {
                    if request.url().path() != "/watch" {
                        return request;
                    }
                    let mut out = request;
                    if let Some(id) = query(&out).get("v") {
                        *out.url_mut() =
                            Url::parse(&format!("https://img.youtube.com/vi/{}/0.jpg", &id))
                                .unwrap();
                    }
                    out
                },
            },
        ];
        Self { quirks }
    }
}

impl Quirks {
    /// Apply quirks to a given request. Only the first quirk regex pattern
    /// matching the URL will be applied. The rest will be discarded for
    /// simplicity reasons. This limitation might be lifted in the future.
    pub(crate) fn apply(&self, request: Request) -> Request {
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
    use pretty_assertions::assert_eq;
    use reqwest::{Request, Url};

    use super::{Quirks, GOOGLEBOT};

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
    fn test_twitter_request() {
        let url = Url::parse("https://twitter.com/zarfeblong/status/1339742840142872577").unwrap();
        let request = Request::new(Method::GET, url.clone());
        let modified = Quirks::default().apply(request);

        assert_eq!(
            modified.headers().get(header::USER_AGENT).unwrap(),
            &GOOGLEBOT
        );
        assert_eq!(MockRequest(modified), MockRequest::new(Method::HEAD, url));
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
    fn test_non_video_youtube_url_untouched() {
        let url = Url::parse("https://www.youtube.com/channel/UCaYhcUwRBNscFNUKTjgPFiA").unwrap();
        let request = Request::new(Method::GET, url.clone());
        let modified = Quirks::default().apply(request);

        assert_eq!(MockRequest(modified), MockRequest::new(Method::GET, url));
    }

    #[test]
    fn test_no_quirk_applied() {
        let url = Url::parse("https://endler.dev").unwrap();
        let request = Request::new(Method::GET, url.clone());
        let modified = Quirks::default().apply(request);

        assert_eq!(MockRequest(modified), MockRequest::new(Method::GET, url));
    }
}
