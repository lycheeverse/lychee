mod excludes;
mod includes;

pub use excludes::Excludes;
pub use includes::Includes;

use crate::uri::Uri;

/// A generic URI filter
/// Used to decide if a given URI should be checked or skipped
#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub(crate) includes: Includes,
    pub(crate) excludes: Excludes,
    pub(crate) scheme: Option<String>,
}

impl Filter {
    #[must_use]
    pub fn new(
        includes: Option<Includes>,
        excludes: Option<Excludes>,
        scheme: Option<String>,
    ) -> Self {
        Filter {
            includes: includes.unwrap_or_default(),
            excludes: excludes.unwrap_or_default(),
            scheme,
        }
    }

    #[must_use]
    pub fn is_excluded(&self, uri: &Uri) -> bool {
        // Skip mail?
        if self.excludes.is_mail_excluded() && uri.scheme() == "mailto" {
            return true;
        }
        // Skip specific IP address?
        if self.excludes.ip(&uri) {
            return true;
        }

        let input = uri.as_str();
        if self.includes.is_empty() {
            if self.excludes.is_empty() {
                // No regex includes/excludes at all?
                // Not excluded unless it's a known false positive
                return Excludes::is_false_positive(input);
            }
        } else if self.includes.regex(input) {
            // Included explicitly (Includes take precedence over excludes)
            return false;
        }
        // Exclude well-known false-positives.
        // This is done after checking includes to allow for user-overwrites.
        if Excludes::is_false_positive(uri.as_str()) {
            return true;
        }
        if self.excludes.is_empty() {
            if !self.includes.is_empty() {
                // In case we have includes and no excludes,
                // skip everything that was not included
                return true;
            }
        } else if self.excludes.regex(input) {
            // Excluded explicitly
            return true;
        }

        // URI scheme excluded?
        matches!(self.scheme, Some(ref scheme) if scheme != uri.scheme())
    }
}

#[cfg(test)]
mod test {
    use regex::RegexSet;
    use reqwest::Url;
    use url::Host;

    use super::{Excludes, Filter, Includes};
    use crate::test_utils::{mail, website};

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

    macro_rules! assert_ip_address {
        (v4: $ip:expr, $predicate:tt) => {
            let res = if let Host::Ipv4(ipv4) = Url::parse($ip).map_err(|_| ())?.host().ok_or(())? {
                ipv4.$predicate()
            } else {
                false
            };
            std::assert!(res);
        };
        (v6: $ip:expr, $predicate:tt) => {
            let res = if let Host::Ipv6(ipv6) = Url::parse($ip).map_err(|_| ())?.host().ok_or(())? {
                ipv6.$predicate()
            } else {
                false
            };
            std::assert!(res);
        };
    }

    #[test]
    fn test_const_sanity() -> Result<(), ()> {
        assert_ip_address!(v4: V4_PRIVATE_CLASS_A, is_private);
        assert_ip_address!(v4: V4_PRIVATE_CLASS_B, is_private);
        assert_ip_address!(v4: V4_PRIVATE_CLASS_C, is_private);

        assert_ip_address!(v4: V4_LOOPBACK, is_loopback);
        assert_ip_address!(v6: V6_LOOPBACK, is_loopback);

        assert_ip_address!(v4: V4_LINK_LOCAL, is_link_local);

        Ok(())
    }

    #[test]
    fn test_includes_and_excludes_empty() {
        // This is the pre-configured, empty set of excludes for a client
        // In this case, only the requests matching the include set will be checked
        let filter = Filter::default();

        assert!(!filter.is_excluded(&website("https://example.org")));
    }

    #[test]
    fn test_false_positives() {
        let filter = Filter::default();

        assert!(filter.is_excluded(&website("http://www.w3.org/1999/xhtml")));
        assert!(!filter.is_excluded(&website("https://example.org")));
    }

    #[test]
    fn test_overwrite_false_positives() {
        let includes = Includes {
            regex: Some(RegexSet::new(&[r"http://www.w3.org/1999/xhtml"]).unwrap()),
        };
        let filter = Filter {
            includes,
            ..Filter::default()
        };
        assert!(!filter.is_excluded(&website("http://www.w3.org/1999/xhtml")));
    }

    #[test]
    fn test_include_regex() {
        let includes = Includes {
            regex: Some(RegexSet::new(&[r"foo.example.org"]).unwrap()),
        };
        let filter = Filter {
            includes,
            ..Filter::default()
        };

        // Only the requests matching the include set will be checked
        assert!(!filter.is_excluded(&website("https://foo.example.org")));
        assert!(filter.is_excluded(&website("https://bar.example.org")));
        assert!(filter.is_excluded(&website("https://example.org")));
    }

    #[test]
    fn test_exclude_mail() {
        let excludes = Excludes {
            mail: true,
            ..Excludes::default()
        };
        let filter = Filter {
            excludes,
            ..Filter::default()
        };

        assert!(filter.is_excluded(&mail("mail@example.org")));
        assert!(filter.is_excluded(&mail("foo@bar.dev")));
        assert!(!filter.is_excluded(&website("http://bar.dev")));
    }

    #[test]
    fn test_exclude_regex() {
        let excludes = Excludes {
            regex: Some(
                RegexSet::new(&[r"github.com", r"[a-z]+\.(org|net)", r"@example.org"]).unwrap(),
            ),
            ..Excludes::default()
        };
        let filter = Filter {
            excludes,
            ..Filter::default()
        };

        assert!(filter.is_excluded(&website("http://github.com")));
        assert!(filter.is_excluded(&website("http://exclude.org")));
        assert!(filter.is_excluded(&mail("mail@example.org")));

        assert!(!filter.is_excluded(&website("http://bar.dev")));
        assert!(!filter.is_excluded(&mail("foo@bar.dev")));
    }
    #[test]
    fn test_exclude_include_regex() {
        let includes = Includes {
            regex: Some(RegexSet::new(&[r"foo.example.org"]).unwrap()),
        };
        let excludes = Excludes {
            regex: Some(RegexSet::new(&[r"example.org"]).unwrap()),
            ..Default::default()
        };
        let filter = Filter {
            includes,
            excludes,
            ..Filter::default()
        };

        // Includes take preference over excludes
        assert!(!filter.is_excluded(&website("https://foo.example.org")),);

        assert!(filter.is_excluded(&website("https://example.org")));
        assert!(filter.is_excluded(&website("https://bar.example.org")));
    }

    #[test]
    fn test_excludes_no_private_ips_by_default() {
        let filter = Filter::default();

        assert!(!filter.is_excluded(&website(V4_PRIVATE_CLASS_A)));
        assert!(!filter.is_excluded(&website(V4_PRIVATE_CLASS_B)));
        assert!(!filter.is_excluded(&website(V4_PRIVATE_CLASS_C)));
        assert!(!filter.is_excluded(&website(V4_LINK_LOCAL)));
        assert!(!filter.is_excluded(&website(V4_LOOPBACK)));
        assert!(!filter.is_excluded(&website(V6_LOOPBACK)));
    }

    #[test]
    fn test_exclude_private_ips() {
        let filter = Filter {
            excludes: Excludes {
                private_ips: true,
                ..Excludes::default()
            },
            ..Filter::default()
        };

        assert!(filter.is_excluded(&website(V4_PRIVATE_CLASS_A)));
        assert!(filter.is_excluded(&website(V4_PRIVATE_CLASS_B)));
        assert!(filter.is_excluded(&website(V4_PRIVATE_CLASS_C)));
    }

    #[test]
    fn test_exclude_link_local() {
        let filter = Filter {
            excludes: Excludes {
                link_local_ips: true,
                ..Excludes::default()
            },
            ..Filter::default()
        };

        assert!(filter.is_excluded(&website(V4_LINK_LOCAL)));
    }

    #[test]
    fn test_exclude_loopback() {
        let filter = Filter {
            excludes: Excludes {
                loopback_ips: true,
                ..Excludes::default()
            },
            ..Filter::default()
        };

        assert!(filter.is_excluded(&website(V4_LOOPBACK)));
        assert!(filter.is_excluded(&website(V6_LOOPBACK)));
    }

    #[test]
    fn test_exclude_ip_v4_mapped_ip_v6_not_supported() {
        let filter = Filter {
            excludes: Excludes {
                private_ips: true,
                link_local_ips: true,
                ..Excludes::default()
            },
            ..Filter::default()
        };

        // if these were pure IPv4, we would exclude
        assert!(!filter.is_excluded(&website(V6_MAPPED_V4_PRIVATE_CLASS_A)));
        assert!(!filter.is_excluded(&website(V6_MAPPED_V4_LINK_LOCAL)));
    }
}
