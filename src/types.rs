use crate::{collector::Input, uri::Uri};
use anyhow::anyhow;
use std::{collections::HashSet, convert::TryFrom, fmt::Display};

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Request {
    pub uri: Uri,
    pub source: Input,
}

impl Request {
    pub fn new(uri: Uri, source: Input) -> Self {
        Request { uri, source }
    }
}

impl Display for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.uri, self.source)
    }
}

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
    pub source: Input,
}

impl Response {
    pub fn new(uri: Uri, status: Status, source: Input) -> Self {
        Response {
            uri,
            status,
            source,
        }
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
    Timeout(Option<http::StatusCode>),
    /// Got redirected to different resource
    Redirected(http::StatusCode),
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
            Status::Redirected(statuscode)
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
            Status::Timeout(e.status())
        } else {
            Status::Error(e.to_string())
        }
    }
}

#[cfg(test)]
mod test {
    use crate::test_utils::website;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_uri_host_ip_v4() {
        let uri = website("http://127.0.0.1");
        let ip = uri.host_ip().expect("Expected a valid IPv4");
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    }

    #[test]
    fn test_uri_host_ip_v6() {
        let uri = website("https://[2020::0010]");
        let ip = uri.host_ip().expect("Expected a valid IPv6");
        assert_eq!(
            ip,
            IpAddr::V6(Ipv6Addr::new(0x2020, 0, 0, 0, 0, 0, 0, 0x10))
        );
    }

    #[test]
    fn test_uri_host_ip_no_ip() {
        let uri = website("https://some.cryptic/url");
        let ip = uri.host_ip();
        assert!(ip.is_none());
    }
}
