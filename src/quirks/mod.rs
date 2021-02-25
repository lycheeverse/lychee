#[macro_use]
mod headers_macro;

use headers::HeaderMap;
use http::{header::USER_AGENT, Method};
use regex::Regex;
use reqwest::{Request, Url};

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

impl Quirk {
    fn matches(&self, url: &Url) -> bool {
            self.pattern.is_match(url.as_str())
    }

    fn apply(&self, request: &mut Request) -> &mut Request {
        &mut request
    }
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

    fn matches(&self, url: &Url) -> Vec<Quirk> {
        let mut matching = vec![];
        for quirk in &self.quirks {
            if quirk.pattern.is_match(url.as_str()) {
                matching.push(quirk.clone());
            }
        }
        matching
    }

    pub fn apply(&self, request: Request) -> Request {
        let mut request = request.clone();
        // let mut req_method = self.method.clone();
        // let mut req_url = url.to_owned();
        // let mut req_headers = None;

        for quirk in self.matches(request.url()) {
            println!("Applying quirk: {:?}", quirk);
            request = quirk.apply(request);
            // if let Some(rewrite) = quirk.rewrite {
            //     req_url = rewrite(url.to_owned());
            // }
            // if let Some(method) = quirk.method {
            //     req_method = method;
            // }
            // req_headers = quirk.headers;
        }

        // if let Some(headers) = req_headers {
        //     request = request.headers(headers);
        // }
        request
    }
}
