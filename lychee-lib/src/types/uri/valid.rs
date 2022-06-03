use std::{convert::TryFrom, fmt::Display, net::IpAddr};

use fast_chemail::parse_email;
use ip_network::Ipv6Network;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{ErrorKind, Result};

use super::raw::RawUri;

/// Lychee's own representation of a URI, which encapsulates all supported
/// formats.
///
/// If the scheme is `mailto`, it's a mail address.
/// Otherwise it's treated as a website URL.
#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Uri {
    /// Website URL or mail address
    pub(crate) url: Url,
}

impl Uri {
    /// Returns the string representation of the `Uri`.
    ///
    /// If it's an email address, returns the string with scheme stripped.
    /// Otherwise returns the string as-is.
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.url.as_ref().trim_start_matches("mailto:")
    }

    #[inline]
    #[must_use]
    /// Returns the scheme of the URI (e.g. `http` or `mailto`)
    pub fn scheme(&self) -> &str {
        self.url.scheme()
    }

    #[inline]
    /// Changes this URL's scheme.
    pub(crate) fn set_scheme(&mut self, scheme: &str) -> std::result::Result<(), ()> {
        self.url.set_scheme(scheme)
    }

    #[inline]
    #[must_use]
    /// Returns the domain of the URI (e.g. `example.com`)
    pub fn domain(&self) -> Option<&str> {
        self.url.domain()
    }

    #[inline]
    #[must_use]
    /// Unless this URL is cannot-be-a-base,
    /// return an iterator of '/' slash-separated path segments,
    /// each as a percent-encoded ASCII string.
    ///
    /// Return `None` for cannot-be-a-base URLs.
    pub fn path_segments(&self) -> Option<std::str::Split<char>> {
        self.url.path_segments()
    }

    #[must_use]
    /// Returns the IP address (either IPv4 or IPv6) of the URI,
    /// or `None` if it is a domain
    pub fn host_ip(&self) -> Option<IpAddr> {
        match self.url.host()? {
            url::Host::Domain(_) => None,
            url::Host::Ipv4(v4_addr) => Some(v4_addr.into()),
            url::Host::Ipv6(v6_addr) => Some(v6_addr.into()),
        }
    }

    #[inline]
    #[must_use]
    /// Check if the URI is a valid mail address
    pub fn is_mail(&self) -> bool {
        self.scheme() == "mailto"
    }

    #[inline]
    #[must_use]
    /// Check if the URI is a file
    pub fn is_file(&self) -> bool {
        self.scheme() == "file"
    }

    #[inline]
    #[must_use]
    /// Returns `true` if this is a loopback address.
    ///
    /// ## IPv4
    ///
    /// This is a loopback address (`127.0.0.0/8`).
    ///
    /// This property is defined by [IETF RFC 1122].
    ///
    /// ## IPv6
    ///
    /// This is the loopback address (`::1`), as defined in [IETF RFC 4291 section 2.5.3].
    ///
    /// [IETF RFC 1122]: https://tools.ietf.org/html/rfc1122
    /// [IETF RFC 4291 section 2.5.3]: https://tools.ietf.org/html/rfc4291#section-2.5.3
    pub fn is_loopback(&self) -> bool {
        match self.url.host() {
            Some(url::Host::Ipv4(addr)) => addr.is_loopback(),
            Some(url::Host::Ipv6(addr)) => addr.is_loopback(),
            _ => false,
        }
    }

    #[inline]
    #[must_use]
    /// Returns `true` if this is a private IPv4 address, a unique local IPv6 address (`fc00::/7`).
    ///
    /// # IPv4
    ///
    /// The private address ranges are defined in [IETF RFC 1918] and include:
    ///
    ///  - `10.0.0.0/8`
    ///  - `172.16.0.0/12`
    ///  - `192.168.0.0/16`
    ///
    /// # IPv6
    ///
    /// Unique local address is defined in [IETF RFC 4193].
    ///
    /// ## Note
    ///
    /// Unicast site-local network was defined in [IETF RFC 4291], but was fully deprecated in
    /// [IETF RFC 3879]. So it is **NOT** considered as private on this purpose.
    ///
    /// [IETF RFC 1918]: https://tools.ietf.org/html/rfc1918
    /// [IETF RFC 4193]: https://tools.ietf.org/html/rfc4193
    /// [IETF RFC 4291]: https://tools.ietf.org/html/rfc4291
    /// [IETF RFC 3879]: https://tools.ietf.org/html/rfc3879
    pub fn is_private(&self) -> bool {
        match self.url.host() {
            Some(url::Host::Ipv4(addr)) => addr.is_private(),
            Some(url::Host::Ipv6(addr)) => Ipv6Network::from(addr).is_unique_local(),
            _ => false,
        }
    }

    #[inline]
    #[must_use]
    /// Returns `true` if the address is a link-local IPv4 address (`169.254.0.0/16`),
    /// or an IPv6 unicast address with link-local scope (`fe80::/10`).
    ///
    /// # IPv4
    ///
    /// Link-local address is defined by [IETF RFC 3927].
    ///
    /// # IPv6
    ///
    /// Unicast address with link-local scope is defined in [IETF RFC 4291].
    ///
    /// [IETF RFC 3927]: https://tools.ietf.org/html/rfc3927
    /// [IETF RFC 4291]: https://tools.ietf.org/html/rfc4291
    pub fn is_link_local(&self) -> bool {
        match self.url.host() {
            Some(url::Host::Ipv4(addr)) => addr.is_link_local(),
            Some(url::Host::Ipv6(addr)) => Ipv6Network::from(addr).is_unicast_link_local(),
            _ => false,
        }
    }
}

impl AsRef<str> for Uri {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<Url> for Uri {
    fn from(url: Url) -> Self {
        Self { url }
    }
}

impl TryFrom<String> for Uri {
    type Error = ErrorKind;

    fn try_from(s: String) -> Result<Self> {
        Uri::try_from(s.as_ref())
    }
}

impl TryFrom<&str> for Uri {
    type Error = ErrorKind;

    fn try_from(s: &str) -> Result<Self> {
        let s = s.trim_start_matches("mailto:");
        // Silently ignore mail parse errors as they are very common and expected for most URIs
        if parse_email(s).is_err() {
            match Url::parse(s) {
                Ok(uri) => Ok(uri.into()),
                Err(url_err) => Err(ErrorKind::ParseUrl(url_err, s.to_owned())),
            }
        } else {
            Ok(Url::parse(&format!("mailto:{s}")).unwrap().into())
        }
    }
}

impl TryFrom<RawUri> for Uri {
    type Error = ErrorKind;

    fn try_from(raw_uri: RawUri) -> Result<Self> {
        let s = raw_uri.text;
        Uri::try_from(s.as_ref())
    }
}

impl Display for Uri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{mail, website};
    use std::{
        convert::TryFrom,
        net::{IpAddr, Ipv4Addr, Ipv6Addr},
    };

    #[test]
    fn test_ipv4_uri_is_loopback() {
        let uri = Uri::try_from("http://127.0.0.0").unwrap();
        assert!(uri.is_loopback());
    }

    #[test]
    fn test_ipv6_uri_is_loopback() {
        let uri = Uri::try_from("https://[::1]").unwrap();
        assert!(uri.is_loopback());
    }

    #[test]
    fn test_uri_from_str() {
        assert!(Uri::try_from("").is_err());
        assert_eq!(
            Uri::try_from("https://example.com"),
            Ok(website("https://example.com"))
        );
        assert_eq!(
            Uri::try_from("https://example.com/@test/testing"),
            Ok(website("https://example.com/@test/testing"))
        );
        assert_eq!(
            Uri::try_from("mail@example.com"),
            Ok(mail("mail@example.com"))
        );
        assert_eq!(
            Uri::try_from("mailto:mail@example.com"),
            Ok(mail("mail@example.com"))
        );
    }

    #[test]
    fn test_uri_host_ip_v4() {
        assert_eq!(
            website("http://127.0.0.1").host_ip(),
            Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))
        );
    }

    #[test]
    fn test_uri_host_ip_v6() {
        assert_eq!(
            website("https://[2020::0010]").host_ip(),
            Some(IpAddr::V6(Ipv6Addr::new(0x2020, 0, 0, 0, 0, 0, 0, 0x10)))
        );
    }

    #[test]
    fn test_uri_host_ip_no_ip() {
        assert!(website("https://some.cryptic/url").host_ip().is_none());
    }

    #[test]
    fn test_localhost() {
        assert_eq!(
            website("http://127.0.0.1").host_ip(),
            Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))
        );
    }
}
