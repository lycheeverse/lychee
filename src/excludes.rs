use std::net::IpAddr;

use regex::RegexSet;

use crate::Uri;

/// Exclude configuration for the link checker.
/// You can ignore links based on regex patterns or pre-defined IP ranges.
#[derive(Clone, Debug)]
pub struct Excludes {
    pub regex: Option<RegexSet>,
    /// Example: 192.168.0.1
    pub private_ips: bool,
    /// Example: 169.254.0.0
    pub link_local_ips: bool,
    /// For IPv4: 127.0. 0.1/8
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
    pub fn regex(&self, input: &str) -> bool {
        if let Some(excludes) = &self.regex {
            if excludes.is_match(input) {
                return true;
            }
        }
        false
    }

    pub fn ip(&self, uri: &Uri) -> bool {
        if let Some(ipaddr) = uri.host_ip() {
            if self.loopback_ips && ipaddr.is_loopback() {
                return true;
            }

            // Note: in a pathological case, an IPv6 address can be IPv4-mapped
            //       (IPv4 address embedded in a IPv6).  We purposefully
            //       don't deal with it here, and assume if an address is IPv6,
            //       we shouldn't attempt to map it to IPv4.
            //       See: https://tools.ietf.org/html/rfc4291#section-2.5.5.2
            if let IpAddr::V4(v4addr) = ipaddr {
                if self.private_ips && v4addr.is_private() {
                    return true;
                }
                if self.link_local_ips && v4addr.is_link_local() {
                    return true;
                }
            }
        }

        false
    }

    pub fn is_mail_excluded(&self) -> bool {
        self.mail
    }

    pub fn is_empty(&self) -> bool {
        match &self.regex {
            None => true,
            Some(regex_set) => regex_set.is_empty(),
        }
    }
}
