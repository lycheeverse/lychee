mod excludes;
mod includes;

pub use excludes::Excludes;
pub use includes::Includes;

use crate::uri::Uri;
use crate::Request;

/// A generic URI filter
/// Used to decide if a given URI should be checked or skipped
#[derive(Clone, Debug)]
pub struct Filter {
    includes: Includes,
    excludes: Excludes,
    scheme: Option<String>,
}

impl Filter {
    pub fn new(
        includes: Option<Includes>,
        excludes: Option<Excludes>,
        scheme: Option<String>,
    ) -> Self {
        let includes = match includes {
            Some(includes) => includes,
            None => Includes::default(),
        };
        let excludes = match excludes {
            Some(excludes) => excludes,
            None => Excludes::default(),
        };
        Filter {
            includes,
            excludes,
            scheme,
        }
    }

    pub fn excluded(&self, request: &Request) -> bool {
        // Skip mail?
        if matches!(request.uri, Uri::Mail(_)) && self.excludes.is_mail_excluded() {
            return true;
        }
        // Skip specific IP address?
        if self.excludes.ip(&request.uri) {
            return true;
        }
        // No regex includes/excludes at all?
        if self.includes.is_empty() && self.excludes.is_empty() {
            return false;
        }
        if self.includes.regex(request.uri.as_str()) {
            // Includes take precedence over excludes
            return false;
        }
        // In case we have includes and no excludes,
        // skip everything that was not included
        if !self.includes.is_empty() && self.excludes.is_empty() {
            return true;
        }

        // We have no includes. Check regex excludes
        if self.excludes.regex(request.uri.as_str()) {
            return true;
        }

        if self.scheme.is_none() {
            return false;
        }
        request.uri.scheme() != self.scheme
    }
}

#[cfg(test)]
mod test {
    // Note: the standard library as of Rust stable 1.47.0 does not expose
    // "link-local" or "private" IPv6 checks.  However, one might argue
    // that these concepts do exist in IPv6, albeit the naming is different.
    // See: https://en.wikipedia.org/wiki/Link-local_address#IPv6
    // See: https://en.wikipedia.org/wiki/Private_network#IPv6
    // See: https://doc.rust-lang.org/stable/std/net/struct.Ipv6Addr.html#method.is_unicast_link_local
    const V4_PRIVATE_CLASS_A: &str = "http://10.0.0.1";
    const V4_PRIVATE_CLASS_B: &str = "http://172.16.0.1";
    const V4_PRIVATE_CLASS_C: &str = "http://192.168.0.1";

    const V4_LOOPBACK: &str = "http://127.0.0.1";
    const V6_LOOPBACK: &str = "http://[::1]";

    const V4_LINK_LOCAL: &str = "http://169.254.0.1";

    // IPv4-Mapped IPv6 addresses (IPv4 embedded in IPv6)
    const V6_MAPPED_V4_PRIVATE_CLASS_A: &str = "http://[::ffff:10.0.0.1]";
    const V6_MAPPED_V4_LINK_LOCAL: &str = "http://[::ffff:169.254.0.1]";

    use regex::RegexSet;
    use reqwest::Url;

    use super::*;

    use crate::{test_utils::website, Input};

    /// Helper method to convert a string into a Request
    /// Note: This panics on error, so it should only be used for testing
    pub fn request(url: &str) -> Request {
        Request::new(website(url), Input::Stdin, 0)
    }

    #[test]
    fn test_const_sanity() {
        let get_host = |s| {
            Url::parse(s)
                .expect("Expected valid URL")
                .host()
                .expect("Expected host address")
                .to_owned()
        };
        let into_v4 = |host| match host {
            url::Host::Ipv4(ipv4) => ipv4,
            _ => panic!("Not IPv4"),
        };
        let into_v6 = |host| match host {
            url::Host::Ipv6(ipv6) => ipv6,
            _ => panic!("Not IPv6"),
        };

        assert!(into_v4(get_host(V4_PRIVATE_CLASS_A)).is_private());
        assert!(into_v4(get_host(V4_PRIVATE_CLASS_B)).is_private());
        assert!(into_v4(get_host(V4_PRIVATE_CLASS_C)).is_private());

        assert!(into_v4(get_host(V4_LOOPBACK)).is_loopback());
        assert!(into_v6(get_host(V6_LOOPBACK)).is_loopback());

        assert!(into_v4(get_host(V4_LINK_LOCAL)).is_link_local());
    }

    #[test]
    fn test_includes_and_excludes_empty() {
        // This is the pre-configured, empty set of excludes for a client
        // In this case, only the requests matching the include set will be checked
        let includes = Some(Includes::default());
        let excludes = Some(Excludes::default());
        let filter = Filter::new(includes, excludes, None);
        assert_eq!(filter.excluded(&request("https://example.org")), false);
    }

    #[test]
    fn test_include_regex() {
        let includes = Some(Includes {
            regex: Some(RegexSet::new(&[r"foo.example.org"]).unwrap()),
        });
        let filter = Filter::new(includes, None, None);

        // Only the requests matching the include set will be checked
        assert_eq!(filter.excluded(&request("https://foo.example.org")), false);
        assert_eq!(filter.excluded(&request("https://bar.example.org")), true);
        assert_eq!(filter.excluded(&request("https://example.org")), true);
    }

    #[test]
    fn test_exclude_regex() {
        let excludes = Excludes {
            regex: Some(
                RegexSet::new(&[r"github.com", r"[a-z]+\.(org|net)", r"@example.org"]).unwrap(),
            ),
            ..Default::default()
        };
        let filter = Filter::new(None, Some(excludes), None);

        assert_eq!(filter.excluded(&request("http://github.com")), true);
        assert_eq!(filter.excluded(&request("http://exclude.org")), true);
        assert_eq!(
            filter.excluded(&Request::new(
                Uri::Mail("mail@example.org".to_string()),
                Input::Stdin,
                0,
            )),
            true
        );

        assert_eq!(filter.excluded(&request("http://bar.dev")), false);
        assert_eq!(
            filter.excluded(&Request::new(
                Uri::Mail("foo@bar.dev".to_string()),
                Input::Stdin,
                0,
            )),
            false
        );
    }
    #[test]
    fn test_exclude_include_regex() {
        let includes = Some(Includes {
            regex: Some(RegexSet::new(&[r"foo.example.org"]).unwrap()),
        });
        let excludes = Excludes {
            regex: Some(RegexSet::new(&[r"example.org"]).unwrap()),
            ..Default::default()
        };

        let filter = Filter::new(includes, Some(excludes), None);

        // Includes take preference over excludes
        assert_eq!(filter.excluded(&request("https://foo.example.org")), false);

        assert_eq!(filter.excluded(&request("https://example.org")), true);
        assert_eq!(filter.excluded(&request("https://bar.example.org")), true);
    }

    #[test]
    fn test_excludes_no_private_ips_by_default() {
        let filter = Filter::new(None, None, None);

        assert_eq!(filter.excluded(&request(V4_PRIVATE_CLASS_A)), false);
        assert_eq!(filter.excluded(&request(V4_PRIVATE_CLASS_B)), false);
        assert_eq!(filter.excluded(&request(V4_PRIVATE_CLASS_C)), false);
        assert_eq!(filter.excluded(&request(V4_LINK_LOCAL)), false);
        assert_eq!(filter.excluded(&request(V4_LOOPBACK)), false);
        assert_eq!(filter.excluded(&request(V6_LOOPBACK)), false);
    }

    #[test]
    fn test_exclude_private_ips() {
        let mut filter = Filter::new(None, None, None);
        filter.excludes.private_ips = true;

        assert_eq!(filter.excluded(&request(V4_PRIVATE_CLASS_A)), true);
        assert_eq!(filter.excluded(&request(V4_PRIVATE_CLASS_B)), true);
        assert_eq!(filter.excluded(&request(V4_PRIVATE_CLASS_C)), true);
    }

    #[test]
    fn test_exclude_link_local() {
        let mut filter = Filter::new(None, None, None);
        filter.excludes.link_local_ips = true;
        assert_eq!(filter.excluded(&request(V4_LINK_LOCAL)), true);
    }

    #[test]
    fn test_exclude_loopback() {
        let mut filter = Filter::new(None, None, None);
        filter.excludes.loopback_ips = true;

        assert_eq!(filter.excluded(&request(V4_LOOPBACK)), true);
        assert_eq!(filter.excluded(&request(V6_LOOPBACK)), true);
    }

    #[test]
    fn test_exclude_ip_v4_mapped_ip_v6_not_supported() {
        let mut filter = Filter::new(None, None, None);
        filter.excludes.private_ips = true;
        filter.excludes.link_local_ips = true;

        // if these were pure IPv4, we would exclude
        assert_eq!(
            filter.excluded(&request(V6_MAPPED_V4_PRIVATE_CLASS_A)),
            false
        );
        assert_eq!(filter.excluded(&request(V6_MAPPED_V4_LINK_LOCAL)), false);
    }
}
