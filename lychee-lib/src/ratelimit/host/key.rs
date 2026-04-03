use serde::Deserialize;
use std::fmt;
use url::Url;

use crate::ErrorKind;
use crate::types::Result;

/// A type-safe representation of a hostname for rate limiting purposes.
///
/// This extracts and normalizes hostnames from URLs to ensure consistent
/// rate limiting across requests to the same host (domain or IP address).
///
/// # Examples
///
/// ```
/// use lychee_lib::ratelimit::HostKey;
/// use url::Url;
///
/// let url = Url::parse("https://api.github.com/repos/user/repo").unwrap();
/// let host_key = HostKey::try_from(&url).unwrap();
/// assert_eq!(host_key.as_str(), "api.github.com");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct HostKey(String);

impl HostKey {
    /// Get the hostname as a string slice
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the hostname as an owned String
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl TryFrom<&Url> for HostKey {
    type Error = ErrorKind;

    fn try_from(url: &Url) -> Result<Self> {
        let host = url.host_str().ok_or_else(|| ErrorKind::InvalidUrlHost)?;

        // Normalize to lowercase for consistent lookup
        Ok(HostKey(host.to_lowercase()))
    }
}

impl TryFrom<&crate::Uri> for HostKey {
    type Error = ErrorKind;

    fn try_from(uri: &crate::Uri) -> Result<Self> {
        Self::try_from(&uri.url)
    }
}

impl TryFrom<Url> for HostKey {
    type Error = ErrorKind;

    fn try_from(url: Url) -> Result<Self> {
        HostKey::try_from(&url)
    }
}

impl fmt::Display for HostKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for HostKey {
    fn from(host: String) -> Self {
        HostKey(host.to_lowercase())
    }
}

impl From<&str> for HostKey {
    fn from(host: &str) -> Self {
        HostKey(host.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_key_from_url() {
        let url = Url::parse("https://api.github.com/repos/user/repo").unwrap();
        let host_key = HostKey::try_from(&url).unwrap();
        assert_eq!(host_key.as_str(), "api.github.com");
    }

    #[test]
    fn test_host_key_normalization() {
        let url = Url::parse("https://API.GITHUB.COM/repos/user/repo").unwrap();
        let host_key = HostKey::try_from(&url).unwrap();
        assert_eq!(host_key.as_str(), "api.github.com");
    }

    #[test]
    fn test_host_key_subdomain_separation() {
        let api_url = Url::parse("https://api.github.com/").unwrap();
        let www_url = Url::parse("https://www.github.com/").unwrap();

        let api_key = HostKey::try_from(&api_url).unwrap();
        let www_key = HostKey::try_from(&www_url).unwrap();

        assert_ne!(api_key, www_key);
        assert_eq!(api_key.as_str(), "api.github.com");
        assert_eq!(www_key.as_str(), "www.github.com");
    }

    #[test]
    fn test_host_key_from_string() {
        let host_key = HostKey::from("example.com");
        assert_eq!(host_key.as_str(), "example.com");

        let host_key = HostKey::from("EXAMPLE.COM");
        assert_eq!(host_key.as_str(), "example.com");
    }

    #[test]
    fn test_host_key_no_host() {
        let url = Url::parse("file:///path/to/file").unwrap();
        let result = HostKey::try_from(&url);
        assert!(result.is_err());
    }

    #[test]
    fn test_host_key_display() {
        let host_key = HostKey::from("example.com");
        assert_eq!(format!("{host_key}"), "example.com");
    }

    #[test]
    fn test_host_key_hash_equality() {
        use std::collections::HashMap;

        let key1 = HostKey::from("example.com");
        let key2 = HostKey::from("EXAMPLE.COM");

        let mut map = HashMap::new();
        map.insert(key1, "value");

        // Should find the value with normalized key
        assert_eq!(map.get(&key2), Some(&"value"));
    }
}
