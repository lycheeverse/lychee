use crate::uri::Uri;
use anyhow::anyhow;
use std::{collections::HashSet, convert::TryFrom};

/// Specifies how requests to websites will be made
pub(crate) enum RequestMethod {
    GET,
    HEAD,
}

impl TryFrom<String> for RequestMethod {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_ref() {
            "get" => Ok(RequestMethod::GET),
            "head" => Ok(RequestMethod::HEAD),
            _ => Err(anyhow!("Only `get` and `head` allowed, got {}", value)),
        }
    }
}

#[derive(Debug)]
pub struct Response {
    pub uri: Uri,
    pub status: Status,
}

impl Response {
    pub fn new(uri: Uri, status: Status) -> Self {
        Response { uri, status }
    }
}

/// Response status of the request
#[derive(Debug)]
pub enum Status {
    /// Request was successful
    Ok(http::StatusCode),
    /// Request failed with HTTP error code
    Failed(http::StatusCode),
    /// Request timed out
    Timeout,
    /// Got redirected to different resource
    Redirected,
    /// Resource was excluded from checking
    Excluded,
    /// Low-level error while loading resource
    Error(String),
}

impl Status {
    pub fn new(statuscode: http::StatusCode, accepted: Option<HashSet<http::StatusCode>>) -> Self {
        if let Some(true) = accepted.map(|a| a.contains(&statuscode)) {
            Status::Ok(statuscode)
        } else if statuscode.is_success() {
            Status::Ok(statuscode)
        } else if statuscode.is_redirection() {
            Status::Redirected
        } else {
            Status::Failed(statuscode)
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Status::Ok(_))
    }
}

impl From<reqwest::Error> for Status {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Status::Timeout
        } else {
            Status::Error(e.to_string())
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use url::Url;

    #[test]
    fn test_uri_host_ip_v4() {
        let uri =
            Uri::Website(Url::parse("http://127.0.0.1").expect("Expected URI with valid IPv4"));
        let ip = uri.host_ip().expect("Expected a valid IPv4");
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    }

    #[test]
    fn test_uri_host_ip_v6() {
        let uri =
            Uri::Website(Url::parse("https://[2020::0010]").expect("Expected URI with valid IPv6"));
        let ip = uri.host_ip().expect("Expected a valid IPv6");
        assert_eq!(
            ip,
            IpAddr::V6(Ipv6Addr::new(0x2020, 0, 0, 0, 0, 0, 0, 0x10))
        );
    }

    #[test]
    fn test_uri_host_ip_no_ip() {
        let uri = Uri::Website(Url::parse("https://some.cryptic/url").expect("Expected valid URI"));
        let ip = uri.host_ip();
        assert!(ip.is_none());
    }
}
