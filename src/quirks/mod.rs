use http::{header, Method};
use regex::Regex;
use reqwest::{Request, Url};

/// Sadly some pages only return plaintext results if Google is trying to crawl them.
const GOOGLEBOT: &str = "Mozilla/5.0 (compatible; Googlebot/2.1; +http://google.com/bot.html)";

#[derive(Debug, Clone)]
pub struct Quirk {
    pub pattern: Regex,
    pub rewrite: fn(Request) -> Request,
}

#[derive(Debug, Clone)]
pub struct Quirks {
    quirks: Vec<Quirk>,
}

impl Default for Quirks {
    fn default() -> Self {
        let quirks = vec![
            Quirk {
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
                // https://stackoverflow.com/a/19377429/270334
                pattern: Regex::new(r"^(https?://)?(www\.)?(youtube\.com|youtu\.?be)").unwrap(),
                rewrite: |request| {
                    let mut out = request;
                    let original_url = out.url();
                    let urlencoded: String =
                        url::form_urlencoded::byte_serialize(original_url.as_str().as_bytes())
                            .collect();
                    *out.method_mut() = Method::HEAD;
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
    pub fn apply(&self, request: Request) -> Request {
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
    use super::*;

    #[test]
    fn test_twitter_request() {
        let orig = Url::parse("https://twitter.com/zarfeblong/status/1339742840142872577").unwrap();
        let request = Request::new(Method::GET, orig.clone());
        let quirks = Quirks::default();
        let modified = quirks.apply(request);
        assert_eq!(modified.url(), &orig);
        assert_eq!(modified.method(), Method::HEAD);
        assert_eq!(
            modified.headers().get(header::USER_AGENT).unwrap(),
            &GOOGLEBOT
        );
    }

    #[test]
    fn test_youtube_request() {
        let orig = Url::parse("https://www.youtube.com/watch?v=NlKuICiT470&list=PLbWDhxwM_45mPVToqaIZNbZeIzFchsKKQ&index=7").unwrap();
        let request = Request::new(Method::GET, orig);
        let quirks = Quirks::default();
        let modified = quirks.apply(request);
        let expected_url = Url::parse("https://www.youtube.com/oembed?url=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3DNlKuICiT470%26list%3DPLbWDhxwM_45mPVToqaIZNbZeIzFchsKKQ%26index%3D7").unwrap();
        assert_eq!(modified.url(), &expected_url);
        assert_eq!(modified.method(), Method::HEAD);
    }

    #[test]
    fn test_no_quirk_applied() {
        let orig = Url::parse("https://endler.dev").unwrap();
        let request = Request::new(Method::GET, orig.clone());
        let quirks = Quirks::default();
        let modified = quirks.apply(request);
        assert_eq!(modified.url(), &orig);
        assert_eq!(modified.method(), Method::GET);
    }
}
