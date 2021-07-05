mod excludes;
mod includes;

use std::{collections::HashSet, net::IpAddr};

pub use excludes::Excludes;
pub use includes::Includes;

use crate::Uri;

/// Pre-defined exclusions for known false-positives
static FALSE_POSITIVE_PAT: &[&str] = &[r"http://www.w3.org/1999/xhtml"];

#[inline]
#[must_use]
/// The given input is a well-known false-positive, which won't be checked by
/// default. This behavior can be explicitly overwritten by defining an
/// `Include` pattern, which will match on a false positive
pub fn is_false_positive(input: &str) -> bool {
    input == FALSE_POSITIVE_PAT[0]
}

/// A generic URI filter
/// Used to decide if a given URI should be checked or skipped
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Default)]
pub struct Filter {
    /// URIs explicitly included for checking. This takes precedence over excludes
    pub includes: Option<Includes>,
    /// URIs excluded from checking
    pub excludes: Option<Excludes>,
    /// Only check URIs with the given schemes (e.g. `https` and `http`)
    // TODO: includes scheme and excludes scheme
    // TODO: excludes_mail should be merged to excludes scheme
    pub schemes: HashSet<String>,
    /// Example: 192.168.0.1
    pub exclude_private_ips: bool,
    /// Example: 169.254.0.0
    pub exclude_link_local_ips: bool,
    /// For IPv4: 127.0.0.1/8
    /// For IPv6: ::1/128
    pub exclude_loopback_ips: bool,
    /// Example: octocat@github.com
    pub exclude_mail: bool,
}

impl Filter {
    #[inline]
    #[must_use]
    /// Whether e-mails aren't checked
    pub fn is_mail_excluded(&self, uri: &Uri) -> bool {
        uri.is_mail() && self.exclude_mail
    }

    #[must_use]
    /// Whether IP addresses are excluded from checking
    pub fn is_ip_excluded(&self, uri: &Uri) -> bool {
        match uri.host_ip() {
            Some(ip_addr) if self.exclude_loopback_ips && ip_addr.is_loopback() => true,
            // Note: in a pathological case, an IPv6 address can be IPv4-mapped
            //       (IPv4 address embedded in a IPv6).  We purposefully
            //       don't deal with it here, and assume if an address is IPv6,
            //       we shouldn't attempt to map it to IPv4.
            //       See: https://tools.ietf.org/html/rfc4291#section-2.5.5.2
            Some(IpAddr::V4(v4_addr)) if self.exclude_private_ips && v4_addr.is_private() => true,
            Some(IpAddr::V4(v4_addr)) if self.exclude_link_local_ips && v4_addr.is_link_local() => {
                true
            }
            _ => false,
        }
    }

    #[inline]
    #[must_use]
    /// Whether the scheme of the given URI is excluded
    pub fn is_scheme_excluded(&self, uri: &Uri) -> bool {
        if self.schemes.is_empty() {
            return false;
        }
        !self.schemes.contains(uri.scheme())
    }

    #[inline]
    fn is_includes_empty(&self) -> bool {
        !matches!(self.includes, Some(ref includes) if !includes.is_empty())
    }

    #[inline]
    fn is_excludes_empty(&self) -> bool {
        !matches!(self.excludes, Some(ref excludes) if !excludes.is_empty())
    }

    #[inline]
    fn is_includes_match(&self, input: &str) -> bool {
        matches!(self.includes, Some(ref includes) if includes.is_match(input))
    }

    #[inline]
    fn is_excludes_match(&self, input: &str) -> bool {
        matches!(self.excludes, Some(ref excludes) if excludes.is_match(input))
    }

    /// Determine whether a given [`Uri`] should be excluded.
    ///
    /// # Details
    ///
    /// 1. If any of the following conditions are met, the URI is excluded:
    ///   - If it's a mail address and it's configured to ignore mail addresses.
    ///   - If the IP address belongs to a type that is configured to exclude.
    ///   - If the scheme of URI is not the allowed scheme.
    /// 2. Decide whether the URI is *presumably included* or *explicitly included*:
    ///    - When both excludes and includes rules are empty, it's *presumably included* unless
    ///      it's a known false positive.
    ///    - When the includes rules matches the URI, it's *explicitly included*.
    /// 3. When it's a known *false positive* pattern, it's *explicitly excluded*.
    /// 4. Decide whether the URI is *presumably excluded* or *explicitly excluded*:
    ///    - When excludes rules is empty, but includes rules doesn't match the URI, it's
    ///      *presumably excluded*.
    ///    - When the excludes rules matches the URI, it's *explicitly excluded*.
    ///    - When the excludes rules matches the URI, it's *explicitly excluded*.
    #[must_use]
    pub fn is_excluded(&self, uri: &Uri) -> bool {
        // Skip mail address, specific IP, and scheme
        if self.is_mail_excluded(uri) || self.is_ip_excluded(uri) || self.is_scheme_excluded(uri) {
            return true;
        }

        let input = uri.as_str();

        if self.is_includes_empty() {
            if self.is_excludes_empty() {
                // Both excludes and includes rules are empty:
                // *Presumably included* unless it's false positive
                return is_false_positive(input);
            }
        } else if self.is_includes_match(input) {
            // *Explicitly included* (Includes take precedence over excludes)
            return false;
        }

        if is_false_positive(input)
        // Exclude well-known false-positives
        // Performed after checking includes to allow user-overwriddes
                || self.is_excludes_empty()
                // Previous checks imply input is not explicitly included,
                // if excludes rules is empty, then *presumably excluded*
            || self.is_excludes_match(input)
        // If excludes rules matches input, then
        // *explicitly excluded*
        {
            return true;
        }

        false
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

    const V4_LINK_LOCAL_1: &str = "http://169.254.0.1";
    const V4_LINK_LOCAL_2: &str = "http://169.254.10.1:8080";

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

    #[allow(clippy::shadow_unrelated)]
    #[test]
    fn test_const_sanity() -> Result<(), ()> {
        assert_ip_address!(v4: V4_PRIVATE_CLASS_A, is_private);
        assert_ip_address!(v4: V4_PRIVATE_CLASS_B, is_private);
        assert_ip_address!(v4: V4_PRIVATE_CLASS_C, is_private);

        assert_ip_address!(v4: V4_LOOPBACK, is_loopback);
        assert_ip_address!(v6: V6_LOOPBACK, is_loopback);

        assert_ip_address!(v4: V4_LINK_LOCAL_1, is_link_local);
        assert_ip_address!(v4: V4_LINK_LOCAL_2, is_link_local);

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
            regex: RegexSet::new(&[r"http://www.w3.org/1999/xhtml"]).unwrap(),
        };
        let filter = Filter {
            includes: Some(includes),
            ..Filter::default()
        };
        assert!(!filter.is_excluded(&website("http://www.w3.org/1999/xhtml")));
    }

    #[test]
    fn test_include_regex() {
        let includes = Includes {
            regex: RegexSet::new(&[r"foo.example.org"]).unwrap(),
        };
        let filter = Filter {
            includes: Some(includes),
            ..Filter::default()
        };

        // Only the requests matching the include set will be checked
        assert!(!filter.is_excluded(&website("https://foo.example.org")));
        assert!(filter.is_excluded(&website("https://bar.example.org")));
        assert!(filter.is_excluded(&website("https://example.org")));
    }

    #[test]
    fn test_exclude_mail() {
        let filter = Filter {
            exclude_mail: true,
            ..Filter::default()
        };

        assert!(filter.is_excluded(&mail("mail@example.org")));
        assert!(filter.is_excluded(&mail("foo@bar.dev")));
        assert!(!filter.is_excluded(&website("http://bar.dev")));
    }

    #[test]
    fn test_exclude_regex() {
        let excludes = Excludes {
            regex: RegexSet::new(&[r"github.com", r"[a-z]+\.(org|net)", r"@example.org"]).unwrap(),
        };
        let filter = Filter {
            excludes: Some(excludes),
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
            regex: RegexSet::new(&[r"foo.example.org"]).unwrap(),
        };
        let excludes = Excludes {
            regex: RegexSet::new(&[r"example.org"]).unwrap(),
        };
        let filter = Filter {
            includes: Some(includes),
            excludes: Some(excludes),
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
        assert!(!filter.is_excluded(&website(V4_LINK_LOCAL_1)));
        assert!(!filter.is_excluded(&website(V4_LINK_LOCAL_2)));
        assert!(!filter.is_excluded(&website(V4_LOOPBACK)));
        assert!(!filter.is_excluded(&website(V6_LOOPBACK)));
    }

    #[test]
    fn test_exclude_private_ips() {
        let filter = Filter {
            exclude_private_ips: true,
            ..Filter::default()
        };

        assert!(filter.is_excluded(&website(V4_PRIVATE_CLASS_A)));
        assert!(filter.is_excluded(&website(V4_PRIVATE_CLASS_B)));
        assert!(filter.is_excluded(&website(V4_PRIVATE_CLASS_C)));
    }

    #[test]
    fn test_exclude_link_local() {
        let filter = Filter {
            exclude_link_local_ips: true,
            ..Filter::default()
        };

        assert!(filter.is_excluded(&website(V4_LINK_LOCAL_1)));
        assert!(filter.is_excluded(&website(V4_LINK_LOCAL_2)));
    }

    #[test]
    fn test_exclude_loopback() {
        let filter = Filter {
            exclude_loopback_ips: true,
            ..Filter::default()
        };

        assert!(filter.is_excluded(&website(V4_LOOPBACK)));
        assert!(filter.is_excluded(&website(V6_LOOPBACK)));
    }

    #[test]
    fn test_exclude_ip_v4_mapped_ip_v6_not_supported() {
        let filter = Filter {
            exclude_private_ips: true,
            exclude_link_local_ips: true,
            ..Filter::default()
        };

        // if these were pure IPv4, we would exclude
        assert!(!filter.is_excluded(&website(V6_MAPPED_V4_PRIVATE_CLASS_A)));
        assert!(!filter.is_excluded(&website(V6_MAPPED_V4_LINK_LOCAL)));
    }
}
