use std::{collections::HashSet, convert::TryFrom, fmt::Display, net::IpAddr};

use fast_chemail::parse_email;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{ErrorKind, Result};

use super::raw_uri::RawUri;

lazy_static! {
    static ref GITHUB_API_EXCLUDED_ENDPOINTS: HashSet<&'static str> = HashSet::from_iter([
        "about",
        "collections",
        "events",
        "explore",
        "features",
        "issues",
        "marketplace",
        "new",
        "notifications",
        "pricing",
        "pulls",
        "sponsors",
        "topics",
        "watching",
    ]);
}

/// Uri path segments extracted from a Github URL
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct GithubUri {
    /// Organization name
    pub owner: String,
    /// Repository name
    pub repo: String,
    /// e.g. `issues` in `/org/repo/issues`
    pub endpoint: Option<String>,
}

impl GithubUri {
    /// Create a new Github URI without an endpoint
    #[cfg(test)]
    fn new<T: Into<String>>(owner: T, repo: T) -> Self {
        GithubUri {
            owner: owner.into(),
            repo: repo.into(),
            endpoint: None,
        }
    }

    #[cfg(test)]
    fn with_endpoint<T: Into<String>>(owner: T, repo: T, endpoint: T) -> Self {
        GithubUri {
            owner: owner.into(),
            repo: repo.into(),
            endpoint: Some(endpoint.into()),
        }
    }
}

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
    /// Returns the domain of the URI (e.g. `example.org`)
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

    // TODO: Support GitLab etc.
    pub(crate) fn gh_org_and_repo(&self) -> Option<GithubUri> {
        fn remove_suffix<'a>(input: &'a str, suffix: &str) -> &'a str {
            if let Some(stripped) = input.strip_suffix(suffix) {
                return stripped;
            }
            input
        }

        debug_assert!(!self.is_mail(), "Should only be called on a Website type!");

        if matches!(
            self.domain()?,
            "github.com" | "www.github.com" | "raw.githubusercontent.com"
        ) {
            let parts: Vec<_> = self.path_segments()?.collect();
            if parts.len() < 2 {
                // Not a valid org/repo pair.
                // Note: We don't check for exactly 2 here, because the Github
                // API doesn't handle checking individual files inside repos or
                // paths like `github.com/org/repo/issues`, so we are more
                // permissive and only check for repo existence. This is the
                // only way to get a basic check for private repos. Public repos
                // are not affected and should work with a normal check.
                return None;
            }

            let owner = parts[0];
            if GITHUB_API_EXCLUDED_ENDPOINTS.contains(owner) {
                return None;
            }

            let repo = parts[1];
            // If the URL ends with `.git`, assume this is an SSH URL and strip
            // the suffix. See https://github.com/lycheeverse/lychee/issues/384
            let repo = remove_suffix(repo, ".git");

            let endpoint = if parts.len() > 2 && !parts[2].is_empty() {
                Some(parts[2..].join("/"))
            } else {
                None
            };

            return Some(GithubUri {
                owner: owner.to_string(),
                repo: repo.to_string(),
                endpoint,
            });
        }

        None
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

impl TryFrom<RawUri> for Uri {
    type Error = ErrorKind;

    fn try_from(raw_uri: RawUri) -> Result<Self> {
        let s = raw_uri.text;
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
    use crate::{
        test_utils::{mail, website},
        types::uri::GithubUri,
    };

    #[test]
    fn test_uri_from_str() {
        assert!(Uri::try_from("").is_err());
        assert_eq!(
            Uri::try_from("https://example.org"),
            Ok(website("https://example.org"))
        );
        assert_eq!(
            Uri::try_from("https://example.org/@test/testing"),
            Ok(website("https://example.org/@test/testing"))
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
    fn test_localhost() {
        assert_eq!(
            website("http://127.0.0.1").host_ip(),
            Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))
        );
    }

    #[test]
    fn test_github() {
        assert_eq!(
            website("http://github.com/lycheeverse/lychee").gh_org_and_repo(),
            Some(GithubUri::new("lycheeverse", "lychee"))
        );

        assert_eq!(
            website("http://www.github.com/lycheeverse/lychee").gh_org_and_repo(),
            Some(GithubUri::new("lycheeverse", "lychee"))
        );

        assert_eq!(
            website("https://github.com/lycheeverse/lychee").gh_org_and_repo(),
            Some(GithubUri::new("lycheeverse", "lychee"))
        );

        assert_eq!(
            website("https://github.com/lycheeverse/lychee/").gh_org_and_repo(),
            Some(GithubUri::new("lycheeverse", "lychee"))
        );

        assert_eq!(
            website("https://github.com/Microsoft/python-language-server.git").gh_org_and_repo(),
            Some(GithubUri::new("Microsoft", "python-language-server"))
        );

        assert_eq!(
            website("https://github.com/lycheeverse/lychee/foo/bar").gh_org_and_repo(),
            Some(GithubUri::with_endpoint("lycheeverse", "lychee", "foo/bar"))
        );

        assert_eq!(
            website("https://github.com/lycheeverse/lychee/blob/master/NON_EXISTENT_FILE.md")
                .gh_org_and_repo(),
            Some(GithubUri::with_endpoint(
                "lycheeverse",
                "lychee",
                "blob/master/NON_EXISTENT_FILE.md"
            ))
        );
    }

    #[test]
    fn test_github_false_positives() {
        assert!(website("https://github.com/sponsors/analysis-tools-dev ")
            .gh_org_and_repo()
            .is_none());

        assert!(
            website("https://github.com/marketplace/actions/lychee-broken-link-checker")
                .gh_org_and_repo()
                .is_none()
        );

        assert!(website("https://github.com/features/actions")
            .gh_org_and_repo()
            .is_none());

        assert!(
            website("https://pkg.go.dev/github.com/Debian/pkg-go-tools/cmd/pgt-gopath")
                .gh_org_and_repo()
                .is_none()
        );
    }
}
