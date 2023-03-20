use anyhow::{anyhow, Context, Result};
use headers::{authorization::Basic, Authorization, HeaderMap, HeaderName};
use lychee_lib::{remap::Remaps, Base};
use std::{collections::HashSet, time::Duration};
use strum::IntoEnumIterator;

use crate::archive::Archive;

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

/// Parse HTTP headers into a `HeaderMap`
pub(crate) fn parse_headers<T: AsRef<str>>(headers: &[T]) -> Result<HeaderMap> {
    let mut out = HeaderMap::new();
    for header in headers {
        let (key, val) = read_header(header.as_ref())?;
        out.insert(HeaderName::from_bytes(key.as_bytes())?, val.parse()?);
    }
    Ok(out)
}

/// Parse URI remaps
pub(crate) fn parse_remaps(remaps: &[String]) -> Result<Remaps> {
    Remaps::try_from(remaps)
        .context("Remaps must be of the form '<pattern> <uri>' (separated by whitespace)")
}

/// Parse a HTTP basic auth header into username and password
pub(crate) fn parse_basic_auth(auth: &str) -> Result<Authorization<Basic>> {
    let params: Vec<_> = auth.split(':').collect();
    if params.len() != 2 {
        return Err(anyhow!(
            "Basic auth value must be of the form username:password, got {}",
            auth
        ));
    }
    Ok(Authorization::basic(params[0], params[1]))
}

pub(crate) fn parse_base(src: &str) -> Result<Base, lychee_lib::ErrorKind> {
    Base::try_from(src)
}

/// Parse archive provider. If it cannot be parsed, a list of supported
/// providers is returned.
pub(crate) fn parse_archive_provider(provider: &str) -> Result<Archive> {
    Archive::try_from(provider).map_err(|_| {
        anyhow!(
            "Supported providers: {}",
            Archive::iter()
                .map(|variant| variant.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

/// Parse HTTP status codes into a set of `StatusCode`
///
/// Note that this function does not convert the status codes into
/// `StatusCode` but rather into `u16` to avoid the need for
/// `http` as a dependency and to support custom status codes, which are
/// necessary for some websites, which don't adhere to the HTTP spec or IANA.
pub(crate) fn parse_statuscodes(accept: &str) -> Result<HashSet<u16>> {
    let mut statuscodes = HashSet::new();
    for code in accept.split(',') {
        let code: u16 = code.parse::<u16>()?;
        statuscodes.insert(code);
    }
    Ok(statuscodes)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use headers::{HeaderMap, HeaderMapExt};
    use regex::Regex;
    use reqwest::{header, Url};

    use super::*;

    #[test]
    fn test_parse_custom_headers() {
        let mut custom = HeaderMap::new();
        custom.insert(header::ACCEPT, "text/html".parse().unwrap());
        assert_eq!(parse_headers(&["accept=text/html"]).unwrap(), custom);
    }

    #[test]
    fn test_parse_statuscodes() {
        let actual = parse_statuscodes("200,204,301").unwrap();
        let expected = IntoIterator::into_iter([200, 204, 301]).collect::<HashSet<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_basic_auth() {
        let mut expected = HeaderMap::new();
        expected.insert(
            header::AUTHORIZATION,
            "Basic YWxhZGluOmFicmV0ZXNlc2Ftbw==".parse().unwrap(),
        );

        let mut actual = HeaderMap::new();
        let auth_header = parse_basic_auth("aladin:abretesesamo").unwrap();
        actual.typed_insert(auth_header);

        assert_eq!(expected, actual);
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
        assert_eq!(url, Url::try_from("http://127.0.0.1:8080").unwrap());
    }
}
