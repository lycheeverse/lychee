use headers::HeaderMap;
use http::{header::USER_AGENT, Method};
use regex::Regex;
use reqwest::Url;

/// Sadly some pages only return plaintext results if Google is trying to crawl them.
const GOOGLEBOT: &'static str =
    "Mozilla/5.0 (compatible; Googlebot/2.1; +http://google.com/bot.html)";

// Adapted from https://github.com/bluss/maplit for HeaderMaps
macro_rules! headers {
    (@single $($x:tt)*) => (());
    (@count $($rest:expr),*) => (<[()]>::len(&[$(headers!(@single $rest)),*]));

    ($($key:expr => $value:expr,)+) => { headers!($($key => $value),+) };
    ($($key:expr => $value:expr),*) => {
        {
            let _cap = headers!(@count $($key),*);
            let mut _map = headers::HeaderMap::with_capacity(_cap);
            $(
                let _ = _map.insert($key, $value);
            )*
            _map
        }
    };
}

#[derive(Debug, Clone)]
pub struct Quirk {
    pub pattern: Regex,
    pub method: Option<reqwest::Method>,
    pub headers: Option<HeaderMap>,
    pub url: Option<fn(Url) -> Url>,
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
                url: None,
            },
            Quirk {
                // https://stackoverflow.com/a/19377429/270334
                pattern: Regex::new(r"^(https?://)?(www\.)?(youtube\.com|youtu\.?be)").unwrap(),
                method: Some(Method::HEAD),
                headers: None,
                url: Some(|orig_url| {
                    let mut url = Url::parse("https://www.youtube.com/oembed?").unwrap();
                    url.set_query(Some(&format!("url={}", orig_url.as_str())));
                    url
                }),
            },
        ];

        Self { quirks }
    }

    pub fn rewrite(&self, url: &Url) -> Option<Quirk> {
        for quirk in &self.quirks {
            if quirk.pattern.is_match(url.as_str()) {
                return Some(quirk.clone());
            }
        }
        None
    }
}
