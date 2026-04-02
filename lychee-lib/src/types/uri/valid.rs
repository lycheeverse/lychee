use std::{convert::TryFrom, fmt::Display, net::IpAddr};

use email_address::EmailAddress;
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
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.url.as_ref()
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
    /// Returns the path of the URI (e.g. `/path/to/resource`)
    pub fn path(&self) -> &str {
        self.url.path()
    }

    #[inline]
    #[must_use]
    /// Unless this URL is cannot-be-a-base,
    /// return an iterator of '/' slash-separated path segments,
    /// each as a percent-encoded ASCII string.
    ///
    /// Return `None` for cannot-be-a-base URLs.
    pub fn path_segments(&self) -> Option<std::str::Split<'_, char>> {
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

    /// Create a new URI with a `https` scheme
    pub(crate) fn to_https(&self) -> Result<Uri> {
        let mut https_uri = self.clone();
        https_uri
            .set_scheme("https")
            .map_err(|()| ErrorKind::InvalidURI(self.clone()))?;
        Ok(https_uri)
    }

    #[inline]
    #[must_use]
    /// Check if the URI is a valid mail address
    pub fn is_mail(&self) -> bool {
        self.scheme() == "mailto"
    }

    #[inline]
    #[must_use]
    /// Check if the URI is a tel
    pub fn is_tel(&self) -> bool {
        self.scheme() == "tel"
    }

    #[inline]
    #[must_use]
    /// Check if the URI is a file
    pub fn is_file(&self) -> bool {
        self.scheme() == "file"
    }

    #[inline]
    #[must_use]
    /// Check if the URI is a `data` URI
    pub fn is_data(&self) -> bool {
        self.scheme() == "data"
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

    /// Create a new URI from a string
    ///
    /// Note:
    /// We do not handle relative URLs here, as we do not know the base URL.
    /// Furthermore paths also cannot be resolved, as we do not know the file system.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid URI
    ///
    fn try_from(s: &str) -> Result<Self> {
        // Empty strings are accepted when being parsed with `Url::parse`,
        // but we don't want to accept them because there is no clear definition
        // of "validity" in this case.
        if s.is_empty() {
            return Err(ErrorKind::EmptyUrl);
        }

        match Url::parse(s) {
            Ok(uri) => Ok(uri.into()),
            Err(err) => {
                // This could be a relative URL or a mail address or something
                // else entirely. Try the mail address check first, as it's the
                // most common case. Note that we use a relatively weak check
                // here because
                // - `fast_chemail::parse_email` does not accept parameters
                //   (`foo@example?subject=bar`), which are common for website
                //   contact forms
                // - `check_if_email_exists` does additional spam detection,
                //   which we only want to execute when checking the email
                //   addresses, but not when printing all links with `--dump`.
                if EmailAddress::is_valid(s) {
                    // Use the `mailto:` scheme for mail addresses,
                    // which will allow `Url::parse` to parse them.
                    if let Ok(uri) = Url::parse(&format!("mailto:{s}")) {
                        return Ok(uri.into());
                    }
                }

                // We do not handle relative URLs here, as we do not know the base URL.
                Err(ErrorKind::ParseUrl(err, s.to_owned()))
            }
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
    use std::{
        convert::TryFrom,
        net::{IpAddr, Ipv4Addr, Ipv6Addr},
    };
    use test_utils::mail;
    use test_utils::website;

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
    fn test_uri_from_url() {
        assert!(Uri::try_from("").is_err());
        assert_eq!(
            Uri::try_from("https://example.com"),
            Ok(website!("https://example.com"))
        );
        assert_eq!(
            Uri::try_from("https://example.com/@test/testing"),
            Ok(website!("https://example.com/@test/testing"))
        );
    }

    #[test]
    fn test_uri_from_email_str() {
        assert_eq!(
            Uri::try_from("mail@example.com"),
            Ok(mail!("mail@example.com"))
        );
        assert_eq!(
            Uri::try_from("mailto:mail@example.com"),
            Ok(mail!("mail@example.com"))
        );
        assert_eq!(
            Uri::try_from("mail@example.com?foo=bar"),
            Ok(mail!("mail@example.com?foo=bar"))
        );
    }

    #[test]
    fn test_uri_tel() {
        assert_eq!(
            Uri::try_from("tel:1234567890"),
            Ok(Uri::try_from("tel:1234567890").unwrap())
        );
    }

    #[test]
    fn test_uri_host_ip_v4() {
        assert_eq!(
            website!("http://127.0.0.1").host_ip(),
            Some(IpAddr::V4(Ipv4Addr::LOCALHOST))
        );
    }

    #[test]
    fn test_uri_host_ip_v6() {
        assert_eq!(
            website!("https://[2020::0010]").host_ip(),
            Some(IpAddr::V6(Ipv6Addr::new(0x2020, 0, 0, 0, 0, 0, 0, 0x10)))
        );
    }

    #[test]
    fn test_uri_host_ip_no_ip() {
        assert!(website!("https://some.cryptic/url").host_ip().is_none());
    }

    #[test]
    fn test_localhost() {
        assert_eq!(
            website!("http://127.0.0.1").host_ip(),
            Some(IpAddr::V4(Ipv4Addr::LOCALHOST))
        );
    }

    #[test]
    fn test_convert_to_https() {
        assert_eq!(
            website!("http://example.com").to_https().unwrap(),
            website!("https://example.com")
        );

        assert_eq!(
            website!("https://example.com").to_https().unwrap(),
            website!("https://example.com")
        );
    }

    #[test]
    fn test_file_uri() {
        assert!(Uri::try_from("file:///path/to/file").unwrap().is_file());
    }
}
