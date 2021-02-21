use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::{convert::TryFrom, fmt::Display};
use url::Url;

/// Lychee's own representation of a URI, which encapsulates all support formats
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Uri {
    /// Website URL
    Website(Url),
    /// Mail address
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

fn is_internal_link(link: &str) -> bool {
    // The first element should contain the Markdown file link
    // @see https://www.markdownguide.org/basic-syntax/#links
    let anchor_links = link.split("#").next().unwrap_or("");
    return anchor_links.ends_with(".md") | anchor_links.ends_with(".markdown");
}

impl TryFrom<&str> for Uri {
    type Error = anyhow::Error;

    fn try_from(s: &str) -> Result<Self> {
        // Check for internal Markdown links
        let is_link_internal = is_internal_link(s);
        // Remove the `mailto` scheme if it exists
        // to avoid parsing it as a website URL.
        let s = s.trim_start_matches("mailto:");
        if s.contains('@') & !is_link_internal {
            return Ok(Uri::Mail(s.to_string()));
        }
        if let Ok(uri) = Url::parse(s) {
            return Ok(Uri::Website(uri));
        };
        bail!("Cannot convert to Uri")
    }
}

impl Display for Uri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod test {
    use crate::test_utils::website;

    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_uri_from_str() {
        assert!(matches!(Uri::try_from(""), Err(_)));
        assert_eq!(
            Uri::try_from("http://example.com").unwrap(),
            website("http://example.com")
        );
        assert_eq!(
            Uri::try_from("mail@example.com").unwrap(),
            Uri::Mail("mail@example.com".to_string())
        );
        assert_eq!(
            Uri::try_from("mailto:mail@example.com").unwrap(),
            Uri::Mail("mail@example.com".to_string())
        );
    }

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

    #[test]
    fn test_mail() {
        let uri = website("http://127.0.0.1");
        let ip = uri.host_ip().expect("Expected a valid IPv4");
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    }
}
