use header::HeaderValue;
use http::header;
use regex::Regex;
use reqwest::{Request, Url};
use std::collections::HashMap;

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
                // URL with a normal HTTP GET. Previously Googlebot would still
                // receive a plain HTML response (see
                // https://twitter.com/zarfeblong/status/1339742840142872577),
                // but as of today this is no longer the case.
                //
                // Instead we use <nitter.net>, which is an alternative Twitter
                // front-end that serves plain HTML.
                pattern: Regex::new(r"^(https?://)?(www\.)?twitter.com").unwrap(),
                rewrite: |mut request| {
                    request.url_mut().set_host(Some("nitter.net")).unwrap();
                    request
                },
            },
            Quirk {
                pattern: Regex::new(r"^(https?://)?(www\.)?crates.io").unwrap(),
                rewrite: |mut request| {
                    request
                        .headers_mut()
                        .insert(header::ACCEPT, HeaderValue::from_static("text/html"));
                    request
                },
            },
            Quirk {
                // Even missing YouTube videos return a 200, therefore we use
                // the thumbnail endpoint instead
                // (https://img.youtube.com/vi/{video_id}/0.jpg).
                // This works for all known video visibilities.
                // See https://github.com/lycheeverse/lychee/issues/214#issuecomment-819103393)
                pattern: Regex::new(r"^(https?://)?(www\.)?(youtube\.com|youtu\.?be)").unwrap(),
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
    use reqwest::{Request, Url};

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
    fn test_twitter_request() {
        let cases = vec![
            (
                "https://twitter.com/search?q=rustlang",
                "https://nitter.net/search?q=rustlang",
            ),
            ("http://twitter.com/jack", "http://nitter.net/jack"),
            (
                "https://twitter.com/notifications",
                "https://nitter.net/notifications",
            ),
        ];

        for (input, output) in cases {
            let url = Url::parse(input).unwrap();
            let expected = Url::parse(output).unwrap();

            let request = Request::new(Method::GET, url.clone());
            let modified = Quirks::default().apply(request);

            assert_eq!(
                MockRequest(modified),
                MockRequest::new(Method::GET, expected)
            );
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
