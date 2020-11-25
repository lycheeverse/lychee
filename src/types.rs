use crate::extract::FileType;
use crate::options::Config;
use anyhow::{anyhow, Result};
use glob::glob;
use regex::RegexSet;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::{collections::HashSet, convert::TryFrom, fmt::Display};
use tokio::fs::read_to_string;
use tokio::io::{stdin, AsyncReadExt};
use url::Url;

const STDIN: &str = "-";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Uri {
    Website(Url),
    Mail(String),
}

impl Uri {
    pub fn as_str(&self) -> &str {
        match self {
            Uri::Website(url) => url.as_str(),
            Uri::Mail(address) => address.as_str(),
        }
    }

    pub fn scheme(&self) -> Option<String> {
        match self {
            Uri::Website(url) => Some(url.scheme().to_string()),
            Uri::Mail(_address) => None,
        }
    }

    pub fn host_ip(&self) -> Option<IpAddr> {
        match self {
            Self::Website(url) => match url.host()? {
                url::Host::Ipv4(v4_addr) => Some(v4_addr.into()),
                url::Host::Ipv6(v6_addr) => Some(v6_addr.into()),
                _ => None,
            },
            Self::Mail(_) => None,
        }
    }
}

impl Display for Uri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
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

/// Exclude configuration for the link checker.
/// You can ignore links based on regex patterns or pre-defined IP ranges.
#[derive(Clone, Debug)]
pub struct Excludes {
    pub regex: Option<RegexSet>,
    /// Example: 192.168.0.1
    pub private_ips: bool,
    /// Example: 169.254.0.0
    pub link_local_ips: bool,
    /// For IPv4: 127.0. 0.1/8
    /// For IPv6: ::1/128
    pub loopback_ips: bool,
}

impl Excludes {
    pub fn from_options(config: &Config) -> Self {
        // exclude_all_private option turns on all "private" excludes,
        // including private IPs, link-local IPs and loopback IPs
        let enable_exclude = |opt| opt || config.exclude_all_private;

        Self {
            regex: RegexSet::new(&config.exclude).ok(),
            private_ips: enable_exclude(config.exclude_private),
            link_local_ips: enable_exclude(config.exclude_link_local),
            loopback_ips: enable_exclude(config.exclude_loopback),
        }
    }
}

impl Default for Excludes {
    fn default() -> Self {
        Self {
            regex: None,
            private_ips: false,
            link_local_ips: false,
            loopback_ips: false,
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) enum Input {
    RemoteUrl(Url),
    FsGlob(String),
    FsPath(PathBuf),
    Stdin,
}

impl ToString for Input {
    fn to_string(&self) -> String {
        match self {
            Self::RemoteUrl(url) => url.to_string(),
            Self::FsGlob(s) => s.clone(),
            Self::FsPath(p) => p.to_str().unwrap_or_default().to_owned(),
            Self::Stdin => STDIN.to_owned(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct InputContent {
    input: Input,
    file_type: FileType,
    content: String,
}

impl From<&str> for Input {
    fn from(value: &str) -> Self {
        if value == STDIN {
            Self::Stdin
        } else {
            match Url::parse(&value) {
                Ok(url) => Self::RemoteUrl(url),
                Err(_) => Self::FsGlob(value.to_owned()),
            }
        }
    }
}

impl Input {
    async fn get_contents(self) -> Result<Vec<InputContent>> {
        use Input::*;

        let contents = match self {
            RemoteUrl(url) => vec![Self::url_contents(url).await?],
            FsGlob(path_glob) => Self::glob_contents(path_glob).await?,
            FsPath(path) => vec![Self::path_content(&path).await?],
            Stdin => vec![Self::stdin_content().await?],
        };

        Ok(contents)
    }

    async fn url_contents(url: Url) -> Result<InputContent> {
        let res = reqwest::get(url.clone()).await?;
        let content = res.text().await?;
        let input_content = InputContent {
            file_type: FileType::from(&url.as_str()),
            input: Input::RemoteUrl(url),
            content,
        };

        Ok(input_content)
    }

    async fn glob_contents(path_glob: String) -> Result<Vec<InputContent>> {
        let mut contents = vec![];

        for entry in glob(&path_glob)? {
            match entry {
                Ok(path) => {
                    let content = Self::path_content(&path).await?;
                    contents.push(content);
                }
                Err(e) => println!("{:?}", e),
            }
        }

        Ok(contents)
    }

    async fn path_content<P: Into<PathBuf> + AsRef<Path>>(path: P) -> Result<InputContent> {
        let input_content = InputContent {
            file_type: FileType::from(path.as_ref()),
            content: read_to_string(&path).await?,
            input: Input::FsPath(path.into()),
        };

        Ok(input_content)
    }

    async fn stdin_content() -> Result<InputContent> {
        let mut content = String::new();
        let mut stdin = stdin();
        stdin.read_to_string(&mut content).await?;

        let input_content = InputContent {
            input: Input::Stdin,
            content,
            file_type: FileType::Plaintext,
        };

        Ok(input_content)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

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
