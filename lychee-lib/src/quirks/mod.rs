use http::{header, Method};
use regex::Regex;
use reqwest::{Request, Url};

/// Sadly some pages only return plaintext results if Google is trying to crawl them.
const GOOGLEBOT: &str = "Mozilla/5.0 (compatible; Googlebot/2.1; +http://google.com/bot.html)";

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
                // The https://www.youtube.com/oembed API will return 404 for
                // missing videos and can be used to check youtube links.
                // See https://stackoverflow.com/a/19377429/270334
                pattern: Regex::new(r"^(https?://)?(www\.)?(youtube\.com|youtu\.?be)").unwrap(),
                rewrite: |request| {
                    if request.url().path() != "/watch" {
                        return request;
                    }
                    let mut out = request;
                    let original_url = out.url();
                    let urlencoded: String =
                        url::form_urlencoded::byte_serialize(original_url.as_str().as_bytes())
                            .collect();
                    let mut url = Url::parse("https://www.youtube.com/oembed").unwrap();
                    url.set_query(Some(&format!("url={}", urlencoded)));
                    *out.url_mut() = url;
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
    fn test_youtube_video_request() {
        let url = Url::parse("https://www.youtube.com/watch?v=NlKuICiT470&list=PLbWDhxwM_45mPVToqaIZNbZeIzFchsKKQ&index=7").unwrap();
        let request = Request::new(Method::GET, url);
        let modified = Quirks::default().apply(request);
        let expected_url = Url::parse("https://www.youtube.com/oembed?url=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3DNlKuICiT470%26list%3DPLbWDhxwM_45mPVToqaIZNbZeIzFchsKKQ%26index%3D7").unwrap();

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
