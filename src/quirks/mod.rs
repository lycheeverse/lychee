#[macro_use]
mod headers_macro;

use headers::HeaderMap;
use http::{header::USER_AGENT, Method};
use regex::Regex;
use reqwest::Url;

/// Sadly some pages only return plaintext results if Google is trying to crawl them.
const GOOGLEBOT: &'static str =
    "Mozilla/5.0 (compatible; Googlebot/2.1; +http://google.com/bot.html)";

#[derive(Debug, Clone)]
pub struct Quirk {
    pub pattern: Regex,
    pub method: Option<reqwest::Method>,
    pub headers: Option<HeaderMap>,
    pub rewrite: Option<fn(Url) -> Url>,
}

#[derive(Debug, Clone)]
pub struct Quirks {
    quirks: Vec<Quirk>,
}

impl Quirks {
    pub fn init() -> Self {
        let quirks = vec![
            Quirk {
                pattern: Regex::new(r"^(https?://)?(www\.)?twitter.com").unwrap(),
                method: Some(Method::HEAD),
                headers: Some(headers!(
                    USER_AGENT => GOOGLEBOT
                        .parse()
                        .unwrap(),
                )),
                rewrite: None,
            },
            Quirk {
                // https://stackoverflow.com/a/19377429/270334
                pattern: Regex::new(r"^(https?://)?(www\.)?(youtube\.com|youtu\.?be)").unwrap(),
                method: Some(Method::HEAD),
                headers: None,
                rewrite: Some(|orig_url| {
                    let mut url = Url::parse("https://www.youtube.com/oembed?").unwrap();
                    url.set_query(Some(&format!("url={}", orig_url.as_str())));
                    url
                }),
            },
        ];
        Self { quirks }
    }

    pub fn matching(&self, url: &Url) -> Vec<Quirk> {
        let mut matching = vec![];
        for quirk in &self.quirks {
            if quirk.pattern.is_match(url.as_str()) {
                matching.push(quirk.clone());
            }
        }
        matching
    }
}
