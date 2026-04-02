mod regex_filter;

use regex::RegexSet;
use std::collections::HashSet;
use std::sync::LazyLock;

/// Include configuration for the link checker.
/// You can include links based on regex patterns.
pub type Includes = regex_filter::RegexFilter;

/// Exclude configuration for the link checker.
/// You can ignore links based on regex patterns.
pub type Excludes = regex_filter::RegexFilter;

/// You can exclude paths and files based on regex patterns.
pub type PathExcludes = regex_filter::RegexFilter;

use crate::Uri;

/// These domains are explicitly defined by RFC 2606, section 3 Reserved Example
/// Second Level Domain Names for describing example cases and should not be
/// dereferenced as they should not have content.
#[cfg(all(not(test), not(feature = "check_example_domains")))]
static EXAMPLE_DOMAINS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from_iter(["example.com", "example.org", "example.net", "example.edu"])
});

/// We also exclude the example TLDs in section 2 of the same RFC.
/// This exclusion gets subsumed by the `check_example_domains` feature.
#[cfg(all(not(test), not(feature = "check_example_domains")))]
static EXAMPLE_TLDS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from_iter([".test", ".example", ".invalid", ".localhost"]));

// Allow usage of example domains in tests
#[cfg(any(test, feature = "check_example_domains"))]
static EXAMPLE_DOMAINS: LazyLock<HashSet<&'static str>> = LazyLock::new(HashSet::new);

#[cfg(any(test, feature = "check_example_domains"))]
static EXAMPLE_TLDS: LazyLock<HashSet<&'static str>> = LazyLock::new(HashSet::new);

static UNSUPPORTED_DOMAINS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from_iter([
        // Twitter requires an account to view tweets
        // https://news.ycombinator.com/item?id=36540957
        "twitter.com",
    ])
});

/// Pre-defined exclusions for known false-positives
const FALSE_POSITIVE_PAT: &[&str] = &[
    r"^https?://schemas\.openxmlformats\.org",
    r"^https?://schemas\.microsoft\.com",
    r"^https?://schemas\.zune\.net",
    r"^https?://www\.w3\.org/1999/xhtml",
    r"^https?://www\.w3\.org/1999/xlink",
    r"^https?://www\.w3\.org/2000/svg",
    r"^https?://www\.w3\.org/2001/XMLSchema-instance",
    r"^https?://ogp\.me/ns#",
    r"^https?://(.*)/xmlrpc\.php$",
];

static FALSE_POSITIVE_SET: LazyLock<RegexSet> =
    LazyLock::new(|| regex::RegexSet::new(FALSE_POSITIVE_PAT).expect("Failed to create RegexSet"));

/// The given input is a well-known false-positive, which won't be checked by
/// default. This behavior can be explicitly overwritten by defining an
/// `Include` pattern, which will match on a false positive
#[inline]
#[must_use]
pub fn is_false_positive(input: &str) -> bool {
    FALSE_POSITIVE_SET.is_match(input)
}

/// Check if the host belongs to a known example domain as defined in
/// [RFC 2606](https://datatracker.ietf.org/doc/html/rfc2606)
#[inline]
#[must_use]
pub fn is_example_domain(uri: &Uri) -> bool {
    match uri.domain() {
        Some(domain) => {
            // Check if the domain is exactly an example domain or a subdomain of it.
            EXAMPLE_DOMAINS.iter().any(|&example| {
                domain == example
                    || domain
                        .split_once('.')
                        .is_some_and(|(_subdomain, tld_part)| tld_part == example)
            }) || EXAMPLE_TLDS
                .iter()
                .any(|&example_tld| domain.ends_with(example_tld))
        }
        None => {
            // Check if the URI is an email address.
            // e.g. `mailto:mail@example.com`
            // In this case, the domain is part of the path
            if uri.is_mail() {
                EXAMPLE_DOMAINS.iter().any(|tld| uri.path().ends_with(tld))
            } else {
                false
            }
        }
    }
}

/// Check if the host belongs to a known unsupported domain
#[inline]
#[must_use]
pub fn is_unsupported_domain(uri: &Uri) -> bool {
    if let Some(domain) = uri.domain() {
        // It is not enough to use `UNSUPPORTED_DOMAINS.contains(domain)` here
        // as this would not include checks for subdomains, such as
        // `foo.example.com`
        UNSUPPORTED_DOMAINS.iter().any(|tld| domain.ends_with(tld))
    } else {
        false
    }
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
    // TODO: includes_scheme and excludes_scheme
    // TODO: excludes_mail should be an alias for exclude_scheme=mailto
    pub schemes: HashSet<String>,
    /// Example: 192.168.0.1
    pub exclude_private_ips: bool,
    /// Example: 169.254.0.0
    pub exclude_link_local_ips: bool,
    /// For IPv4: 127.0.0.1/8
    /// For IPv6: `::1/128`
    pub exclude_loopback_ips: bool,
    /// Example: octocat@github.com
    pub include_mail: bool,
}

impl Filter {
    #[inline]
    #[must_use]
    /// Whether e-mails aren't checked (which is the default)
    pub fn is_mail_excluded(&self, uri: &Uri) -> bool {
        uri.is_mail() && !self.include_mail
    }

    #[must_use]
    /// Whether the IP address is excluded from checking
    pub fn is_ip_excluded(&self, uri: &Uri) -> bool {
        if (self.exclude_loopback_ips && uri.is_loopback())
            || (self.exclude_private_ips && uri.is_private())
            || (self.exclude_link_local_ips && uri.is_link_local())
        {
            return true;
        }

        false
    }

    #[must_use]
    /// Whether the host is excluded from checking
    pub fn is_host_excluded(&self, uri: &Uri) -> bool {
        // If loopback IPs are excluded, exclude localhost as well, which usually maps to a loopback IP
        self.exclude_loopback_ips && uri.domain() == Some("localhost")
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
    ///   - If it's a mail address and it's not configured to include mail addresses.
    ///   - If the IP address belongs to a type that is configured to exclude.
    ///   - If the host belongs to a type that is configured to exclude.
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
        // Skip mail address, specific IP, specific host and scheme
        if self.is_scheme_excluded(uri)
            || self.is_host_excluded(uri)
            || self.is_ip_excluded(uri)
            || self.is_mail_excluded(uri)
            || uri.is_tel()
            || is_example_domain(uri)
            || is_unsupported_domain(uri)
        {
            return true;
        }

        let input = uri.as_str();

        if self.is_includes_empty() {
            if self.is_excludes_empty() {
                // Both excludes and includes rules are empty:
                // *Presumably included* unless it's a false positive
                return is_false_positive(input);
            }
        } else if self.is_includes_match(input) {
            // *Explicitly included* (Includes take precedence over excludes)
            return false;
        }

        // Exclude well-known false-positives
        // Performed after checking includes to allow user-overwrites
        if is_false_positive(input)
                // Previous checks imply input is not explicitly included.
                // If exclude rules are empty, then *presumably excluded*
                || self.is_excludes_empty()
                // If exclude rules match input, then *explicitly excluded*
                || self.is_excludes_match(input)
        {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use reqwest::Url;
    use test_utils::{mail, website};
    use url::Host;

    use super::{Excludes, Filter, Includes};
    use crate::Uri;

    // Note: the standard library, as of Rust stable 1.47.0, does not expose
    // "link-local" or "private" IPv6 checks. However, one might argue
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
    fn test_exclude_loopback_ips() {
        let filter = Filter {
            exclude_loopback_ips: true,
            ..Filter::default()
        };
        let uri = Uri::try_from("https://[::1]").unwrap();
        assert!(filter.is_excluded(&uri));
        let uri = Uri::try_from("https://127.0.0.1/8").unwrap();
        assert!(filter.is_excluded(&uri));
    }

    #[test]
    fn test_includes_and_excludes_empty() {
        // This is the pre-configured, empty set of excludes for a client.
        // In this case, only the requests matching the include set will be checked
        let filter = Filter::default();

        assert!(!filter.is_excluded(&website!("https://example.com")));
    }

    #[test]
    fn test_false_positives() {
        let filter = Filter::default();

        assert!(filter.is_excluded(&website!("http://www.w3.org/1999/xhtml")));
        assert!(filter.is_excluded(&website!(
            "http://schemas.openxmlformats.org/markup-compatibility/2006"
        )));
        assert!(!filter.is_excluded(&website!("https://example.com")));
    }

    #[test]
    fn test_overwrite_false_positives() {
        let includes = Includes::new([r"http://www.w3.org/1999/xhtml"]).unwrap();
        let filter = Filter {
            includes: Some(includes),
            ..Filter::default()
        };
        assert!(!filter.is_excluded(&website!("http://www.w3.org/1999/xhtml")));
    }

    #[test]
    fn test_include_regex() {
        let includes = Includes::new([r"foo.example.com"]).unwrap();
        let filter = Filter {
            includes: Some(includes),
            ..Filter::default()
        };

        // Only the requests matching the include set will be checked
        assert!(!filter.is_excluded(&website!("https://foo.example.com")));
        assert!(filter.is_excluded(&website!("https://bar.example.com")));
        assert!(filter.is_excluded(&website!("https://example.com")));
    }

    #[test]
    fn test_exclude_mail_by_default() {
        let filter = Filter {
            ..Filter::default()
        };

        assert!(filter.is_excluded(&mail!("mail@example.com")));
        assert!(filter.is_excluded(&mail!("foo@bar.dev")));
        assert!(!filter.is_excluded(&website!("http://bar.dev")));
    }

    #[test]
    fn test_include_mail() {
        let filter = Filter {
            include_mail: true,
            ..Filter::default()
        };

        assert!(!filter.is_excluded(&mail!("mail@example.com")));
        assert!(!filter.is_excluded(&mail!("foo@bar.dev")));
        assert!(!filter.is_excluded(&website!("http://bar.dev")));
    }

    #[test]
    fn test_exclude_regex() {
        let excludes =
            Excludes::new([r"github.com", r"[a-z]+\.(org|net)", r"@example.com"]).unwrap();
        let filter = Filter {
            excludes: Some(excludes),
            ..Filter::default()
        };

        assert!(filter.is_excluded(&website!("https://github.com")));
        assert!(filter.is_excluded(&website!("http://exclude.org")));
        assert!(filter.is_excluded(&mail!("mail@example.com")));

        assert!(!filter.is_excluded(&website!("http://bar.dev")));
        assert!(filter.is_excluded(&mail!("foo@bar.dev")));
    }
    #[test]
    fn test_exclude_include_regex() {
        let includes = Includes::new([r"foo.example.com"]).unwrap();
        let excludes = Excludes::new([r"example.com"]).unwrap();
        let filter = Filter {
            includes: Some(includes),
            excludes: Some(excludes),
            ..Filter::default()
        };

        // Includes take preference over excludes
        assert!(!filter.is_excluded(&website!("https://foo.example.com")),);

        assert!(filter.is_excluded(&website!("https://example.com")));
        assert!(filter.is_excluded(&website!("https://bar.example.com")));
    }

    #[test]
    fn test_excludes_no_private_ips_by_default() {
        let filter = Filter::default();

        assert!(!filter.is_excluded(&website!(V4_PRIVATE_CLASS_A)));
        assert!(!filter.is_excluded(&website!(V4_PRIVATE_CLASS_B)));
        assert!(!filter.is_excluded(&website!(V4_PRIVATE_CLASS_C)));
        assert!(!filter.is_excluded(&website!(V4_LINK_LOCAL_1)));
        assert!(!filter.is_excluded(&website!(V4_LINK_LOCAL_2)));
        assert!(!filter.is_excluded(&website!(V4_LOOPBACK)));
        assert!(!filter.is_excluded(&website!(V6_LOOPBACK)));
        assert!(!filter.is_excluded(&website!("http://localhost")));
    }

    #[test]
    fn test_exclude_private_ips() {
        let filter = Filter {
            exclude_private_ips: true,
            ..Filter::default()
        };

        assert!(filter.is_excluded(&website!(V4_PRIVATE_CLASS_A)));
        assert!(filter.is_excluded(&website!(V4_PRIVATE_CLASS_B)));
        assert!(filter.is_excluded(&website!(V4_PRIVATE_CLASS_C)));
    }

    #[test]
    fn test_exclude_link_local() {
        let filter = Filter {
            exclude_link_local_ips: true,
            ..Filter::default()
        };

        assert!(filter.is_excluded(&website!(V4_LINK_LOCAL_1)));
        assert!(filter.is_excluded(&website!(V4_LINK_LOCAL_2)));
    }

    #[test]
    fn test_exclude_loopback() {
        let filter = Filter {
            exclude_loopback_ips: true,
            ..Filter::default()
        };

        assert!(filter.is_excluded(&website!(V4_LOOPBACK)));
        assert!(filter.is_excluded(&website!(V6_LOOPBACK)));
        assert!(filter.is_excluded(&website!("http://localhost")));
    }

    #[test]
    fn test_exclude_ip_v4_mapped_ip_v6_not_supported() {
        let filter = Filter {
            exclude_private_ips: true,
            exclude_link_local_ips: true,
            ..Filter::default()
        };

        // if these were pure IPv4, we would exclude
        assert!(!filter.is_excluded(&website!(V6_MAPPED_V4_PRIVATE_CLASS_A)));
        assert!(!filter.is_excluded(&website!(V6_MAPPED_V4_LINK_LOCAL)));
    }
}
