use anyhow::{anyhow, Result};
use headers::{authorization::Basic, Authorization, HeaderMap, HeaderName};
use http::StatusCode;
use lychee_lib::remap::Remaps;
use regex::Regex;
use reqwest::Url;
use std::{collections::HashSet, time::Duration};

fn read_header(input: &str) -> Result<(String, String)> {
    let elements: Vec<_> = input.split('=').collect();
    if elements.len() != 2 {
        return Err(anyhow!(
            "Header value should be of the form key=value, got {}",
            input
        ));
    }
    Ok((elements[0].into(), elements[1].into()))
}

pub(crate) const fn parse_duration_secs(secs: usize) -> Duration {
    Duration::from_secs(secs as u64)
}

pub(crate) fn parse_headers<T: AsRef<str>>(headers: &[T]) -> Result<HeaderMap> {
    let mut out = HeaderMap::new();
    for header in headers {
        let (key, val) = read_header(header.as_ref())?;
        out.insert(HeaderName::from_bytes(key.as_bytes())?, val.parse()?);
    }
    Ok(out)
}

pub(crate) fn parse_statuscodes<T: AsRef<str>>(accept: T) -> Result<HashSet<StatusCode>> {
    let mut statuscodes = HashSet::new();
    for code in accept.as_ref().split(',') {
        let code: StatusCode = StatusCode::from_bytes(code.as_bytes())?;
        statuscodes.insert(code);
    }
    Ok(statuscodes)
}

/// Parse URI remaps
pub(crate) fn parse_remaps(remaps: &[String]) -> Result<Remaps> {
    let mut parsed = Vec::new();

    for remap in remaps {
        let params: Vec<_> = remap.split_whitespace().collect();
        if params.len() != 2 {
            return Err(anyhow!(
                "Remap values must be of the form `pattern url`, got {}",
                remap
            ));
        }

        let pattern = Regex::new(params[0])?;
        let url = Url::try_from(params[1])?;
        parsed.push((pattern, url))
    }

    Ok(parsed)
}

pub(crate) fn parse_basic_auth(auth: &str) -> Result<Authorization<Basic>> {
    let params: Vec<_> = auth.split(':').collect();
    if params.len() != 2 {
        return Err(anyhow!(
            "Basic auth value should be of the form username:password, got {}",
            auth
        ));
    }
    Ok(Authorization::basic(params[0], params[1]))
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use headers::{HeaderMap, HeaderMapExt};
    use http::StatusCode;
    use reqwest::header;

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
        let expected = IntoIterator::into_iter([
            StatusCode::OK,
            StatusCode::NO_CONTENT,
            StatusCode::MOVED_PERMANENTLY,
        ])
        .collect::<HashSet<_>>();

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
