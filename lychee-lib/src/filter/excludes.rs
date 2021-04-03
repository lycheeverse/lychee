use regex::RegexSet;
use std::net::IpAddr;

use crate::Uri;

/// Pre-defined exclusions for known false-positives
static FALSE_POSITIVE_PAT: &[&str] = &[r"http://www.w3.org/1999/xhtml"];

/// Exclude configuration for the link checker.
/// You can ignore links based on regex patterns or pre-defined IP ranges.
#[derive(Clone, Debug)]
pub struct Excludes {
    /// User-defined set of excluded regex patterns
    pub regex: Option<RegexSet>,
    /// Example: 192.168.0.1
    pub private_ips: bool,
    /// Example: 169.254.0.0
    pub link_local_ips: bool,
    /// For IPv4: 127.0.0.1/8
    /// For IPv6: ::1/128
    pub loopback_ips: bool,
    /// Example: octocat@github.com
    pub mail: bool,
}

impl Default for Excludes {
    fn default() -> Self {
        Self {
            regex: None,
            private_ips: false,
            link_local_ips: false,
            loopback_ips: false,
            mail: false,
        }
    }
}

impl Excludes {
    #[inline]
    pub fn regex(&self, input: &str) -> bool {
        self.regex.as_ref().map_or(false, |re| re.is_match(input))
    }

    pub fn is_false_positive(input: &str) -> bool {
        input == FALSE_POSITIVE_PAT[0]
    }

    pub fn ip(&self, uri: &Uri) -> bool {
        match uri.host_ip() {
            Some(ip_addr) if self.loopback_ips && ip_addr.is_loopback() => true,
            // Note: in a pathological case, an IPv6 address can be IPv4-mapped
            //       (IPv4 address embedded in a IPv6).  We purposefully
            //       don't deal with it here, and assume if an address is IPv6,
            //       we shouldn't attempt to map it to IPv4.
            //       See: https://tools.ietf.org/html/rfc4291#section-2.5.5.2
            Some(IpAddr::V4(v4_addr)) if self.private_ips && v4_addr.is_private() => true,
            Some(IpAddr::V4(v4_addr)) if self.link_local_ips && v4_addr.is_link_local() => true,
            _ => false,
        }
    }

    #[inline]
    pub fn is_mail_excluded(&self) -> bool {
        self.mail
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.regex.as_ref().map_or(true, RegexSet::is_empty)
    }
}
