use anyhow::{anyhow, Context, Result};
use headers::{HeaderMap, HeaderName};
use http::HeaderValue;
use lychee_lib::{remap::Remaps, Base};
use std::time::Duration;

/// Split a single HTTP header into a (key, value) tuple
fn read_header(input: &str) -> Result<(String, String)> {
    let elements: Vec<_> = input.split('=').collect();
    if elements.len() != 2 {
        return Err(anyhow!(
            "Header value must be of the form key=value, got {}",
            input
        ));
    }
    Ok((elements[0].into(), elements[1].into()))
}

/// Parse seconds into a `Duration`
pub(crate) const fn parse_duration_secs(secs: usize) -> Duration {
    Duration::from_secs(secs as u64)
}

/// Parse a header given in a string format into a `HeaderMap`
///
/// Headers are expected to be in format "key:value".
fn parse_header(header: &str) -> Result<HeaderMap, String> {
    let header: Vec<&str> = header.split(':').collect();
    if header.len() != 2 {
        return Err("Wrong header format (see --help for format)".to_string());
    }

    let (header_name, header_value) = (header[0], header[1]);

    let hn = HeaderName::from_lowercase(header_name.trim().to_lowercase().as_bytes())
        .map_err(|e| e.to_string())?;

    let hv = HeaderValue::from_str(header_value.trim()).map_err(|e| e.to_string())?;

    let mut map = HeaderMap::new();
    map.insert(hn, hv);
    Ok(map)
}

/// Parse URI remaps
pub(crate) fn parse_remaps(remaps: &[String]) -> Result<Remaps> {
    Remaps::try_from(remaps)
        .context("Remaps must be of the form '<pattern> <uri>' (separated by whitespace)")
}

pub(crate) fn parse_base(src: &str) -> Result<Base, lychee_lib::ErrorKind> {
    Base::try_from(src)
}

#[cfg(test)]
mod tests {

    use headers::HeaderMap;
    use regex::Regex;
    use reqwest::header;

    use super::*;

    #[test]
    fn test_parse_custom_headers() {
        let mut custom = HeaderMap::new();
        custom.insert(header::ACCEPT, "text/html".parse().unwrap());
        assert_eq!(parse_headers(&["accept=text/html"]).unwrap(), custom);
    }

    #[test]
    fn test_parse_remap() {
        let remaps =
            parse_remaps(&["https://example.com http://127.0.0.1:8080".to_string()]).unwrap();
        assert_eq!(remaps.len(), 1);
        let (pattern, url) = remaps[0].to_owned();
        assert_eq!(
            pattern.to_string(),
            Regex::new("https://example.com").unwrap().to_string()
        );
        assert_eq!(url, "http://127.0.0.1:8080");
    }
}
