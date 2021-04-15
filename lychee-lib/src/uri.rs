use std::{convert::TryFrom, fmt::Display, net::IpAddr};

use fast_chemail::parse_email;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{ErrorKind, Result};

/// Lychee's own representation of a URI, which encapsulates all support formats.
///
/// If the scheme is `mailto`, it's a mail address.
/// Otherwise it's treated as a website URL.
#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
    pub fn scheme(&self) -> &str {
        self.url.scheme()
    }

    #[inline]
    #[must_use]
    pub fn domain(&self) -> Option<&str> {
        self.url.domain()
    }

    #[inline]
    #[must_use]
    pub fn path_segments(&self) -> Option<std::str::Split<char>> {
        self.url.path_segments()
    }

    #[must_use]
    pub fn host_ip(&self) -> Option<IpAddr> {
        match self.url.host()? {
            url::Host::Domain(_) => None,
            url::Host::Ipv4(v4_addr) => Some(v4_addr.into()),
            url::Host::Ipv6(v6_addr) => Some(v6_addr.into()),
        }
    }

    // TODO: Support GitLab etc.
    pub(crate) fn extract_github(&self) -> Option<(&str, &str)> {
        debug_assert!(!self.is_mail(), "Should only be called on a Website type!");

        // TODO: Support more patterns
        if matches!(
            self.domain()?,
            "github.com" | "www.github.com" | "raw.githubusercontent.com"
        ) {
            let mut path = self.path_segments()?;
            let owner = path.next()?;
            let repo = path.next()?;
            return Some((owner, repo));
        }

        None
    }

    #[inline]
    pub(crate) fn is_mail(&self) -> bool {
        self.scheme() == "mailto"
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
        let s = s.trim_start_matches("mailto:");
        if let Err(mail_err) = parse_email(s) {
            match Url::parse(s) {
                Ok(uri) => Ok(uri.into()),
                Err(url_err) => Err((s.to_owned(), url_err, mail_err).into()),
            }
        } else {
            Ok(Url::parse(&(String::from("mailto:") + s)).unwrap().into())
        }
    }
}

impl TryFrom<&str> for Uri {
    type Error = ErrorKind;

    fn try_from(s: &str) -> Result<Self> {
        let s = s.trim_start_matches("mailto:");
        if let Err(mail_err) = parse_email(s) {
            match Url::parse(s) {
                Ok(uri) => Ok(uri.into()),
                Err(url_err) => Err((s.to_owned(), url_err, mail_err).into()),
            }
        } else {
            Ok(Url::parse(&(String::from("mailto:") + s)).unwrap().into())
        }
    }
}

impl Display for Uri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod test {
    use std::{
        convert::TryFrom,
        net::{IpAddr, Ipv4Addr, Ipv6Addr},
    };

    use pretty_assertions::assert_eq;

    use super::Uri;
    use crate::test_utils::{mail, website};

    #[test]
    fn test_uri_from_str() {
        assert!(Uri::try_from("").is_err());
        assert_eq!(
            Uri::try_from("http://example.org"),
            Ok(website("http://example.org"))
        );
        assert_eq!(
            Uri::try_from("http://example.org/@test/testing"),
            Ok(website("http://example.org/@test/testing"))
        );
        assert_eq!(
            Uri::try_from("mail@example.org"),
            Ok(mail("mail@example.org"))
        );
        assert_eq!(
            Uri::try_from("mailto:mail@example.org"),
            Ok(mail("mail@example.org"))
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
    fn test_mail() {
        assert_eq!(
            website("http://127.0.0.1").host_ip(),
            Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))
        );
    }

    #[test]
    fn test_is_github() {
        assert_eq!(
            website("http://github.com/lycheeverse/lychee").extract_github(),
            Some(("lycheeverse", "lychee"))
        );

        assert_eq!(
            website("http://www.github.com/lycheeverse/lychee").extract_github(),
            Some(("lycheeverse", "lychee"))
        );

        assert_eq!(
            website("https://github.com/lycheeverse/lychee").extract_github(),
            Some(("lycheeverse", "lychee"))
        );

        assert!(
            website("https://pkg.go.dev/github.com/Debian/pkg-go-tools/cmd/pgt-gopath")
                .extract_github()
                .is_none()
        );
    }
}
