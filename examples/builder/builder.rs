use http::StatusCode;
use http::header::{self, HeaderMap};
use lychee_lib::{ClientBuilder, Result};
use regex::RegexSet;
use reqwest::Method;
use std::{collections::HashSet, time::Duration};

#[tokio::main]
#[allow(clippy::trivial_regex)]
async fn main() -> Result<()> {
    // Excludes
    let excludes = Some(RegexSet::new([r"example"]).unwrap());
    // Includes take precedence over excludes
    let includes = Some(RegexSet::new([r"example.com"]).unwrap());

    // Set custom request headers
    let mut headers = HeaderMap::new();
    headers.insert(header::ACCEPT, "text/html".parse().unwrap());

    let accepted = HashSet::from_iter(vec![StatusCode::OK, StatusCode::NO_CONTENT]);

    let client = ClientBuilder::builder()
        .excludes(excludes)
        .includes(includes)
        .max_redirects(3u8)
        .user_agent("custom useragent")
        .allow_insecure(true)
        .custom_headers(headers)
        .method(Method::HEAD)
        .timeout(Duration::from_secs(5))
        .schemes(HashSet::from_iter(vec![
            "http".to_string(),
            "https".to_string(),
        ]))
        .accepted(accepted)
        .build()
        .client()?;

    let response = client.check("https://example.com").await?;
    dbg!(&response);
    assert!(response.status().is_success());
    Ok(())
}
