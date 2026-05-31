use http::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::hash_map::Iter;
use std::time::Duration;

use crate::ratelimit::HostKey;

/// Default number of concurrent requests per host
const DEFAULT_CONCURRENCY: usize = 10;

/// Default interval between requests to the same host
const DEFAULT_REQUEST_INTERVAL: Duration = Duration::from_millis(50);

/// Global rate limiting configuration that applies as defaults to all hosts
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Default maximum concurrent requests per host
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,

    /// Default minimum interval between requests to the same host
    #[serde(default = "default_request_interval", with = "humantime_serde")]
    pub request_interval: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            concurrency: default_concurrency(),
            request_interval: default_request_interval(),
        }
    }
}

/// Default number of concurrent requests per host
const fn default_concurrency() -> usize {
    DEFAULT_CONCURRENCY
}

/// Default interval between requests to the same host
const fn default_request_interval() -> Duration {
    DEFAULT_REQUEST_INTERVAL
}

impl RateLimitConfig {
    /// Create a `RateLimitConfig` from CLI options, using defaults for missing values
    #[must_use]
    pub fn from_options(concurrency: Option<usize>, request_interval: Option<Duration>) -> Self {
        Self {
            concurrency: concurrency.unwrap_or(DEFAULT_CONCURRENCY),
            request_interval: request_interval.unwrap_or(DEFAULT_REQUEST_INTERVAL),
        }
    }
}

/// Per-host configuration overrides
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct HostConfigs(HashMap<HostKey, HostConfig>);

impl HostConfigs {
    /// Built-in, host-specific configuration defaults.
    ///
    /// These are real-world overrides that improve lychee's out-of-the-box
    /// behaviour for hosts that are known to behave unexpectedly with the
    /// default settings (for example, hosts that reject the default
    /// user-agent). This is comparable to [`quirks`](crate::quirks), but for
    /// [`HostConfig`]s.
    ///
    /// These defaults take precedence over the user's *global* defaults, but
    /// not over the user's *per-host* configuration (see [`HostConfigs::merge`]).
    ///
    /// Note that host keys are matched against the exact, lowercased hostname,
    /// so `www.gnu.org` does not cover `gnu.org` (and vice versa).
    ///
    /// To add a new default, insert another entry below. Additions of
    /// real-world host configurations are welcome.
    #[must_use]
    pub fn builtin_defaults() -> HostConfigs {
        // Hosts that only respond as expected to a browser- or curl-like
        // user-agent. See https://github.com/lycheeverse/lychee/issues/1960.
        let host_user_agents = [
            ("www.gnu.org", "Mozilla/5.0"),
            ("developers.redhat.com", "curl/8.4.0"),
        ];

        let mut configs = HashMap::new();
        for (host, user_agent) in host_user_agents {
            let mut headers = HeaderMap::new();
            headers.insert(
                HeaderName::from_static("user-agent"),
                HeaderValue::from_static(user_agent),
            );
            configs.insert(
                HostKey::from(host),
                HostConfig {
                    headers,
                    ..HostConfig::default()
                },
            );
        }

        HostConfigs(configs)
    }

    /// Get a reference to the [`HostConfig`] associated to the [`HostKey`]
    pub(crate) fn get(&self, key: &HostKey) -> Option<&HostConfig> {
        self.0.get(key)
    }

    /// Get the number of [`HostConfig`]s
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if if there are no [`HostConfig`]s
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get the iterator over all elements
    pub(crate) fn iter(&self) -> Iter<'_, HostKey, HostConfig> {
        self.0.iter()
    }

    /// Merge `self` with another `HostConfigs`.
    ///
    /// `self` takes precedence over `other` on a per-field and per-header-key
    /// basis (see [`HostConfig::merge`]).
    #[must_use]
    pub fn merge(mut self, other: HostConfigs) -> HostConfigs {
        for (key, value) in other.0 {
            let value = if let Some(s) = self.0.remove(&key) {
                s.merge(value)
            } else {
                value
            };

            self.0.insert(key, value);
        }

        self
    }
}

impl<'a> IntoIterator for &'a HostConfigs {
    type Item = (&'a HostKey, &'a HostConfig);
    type IntoIter = Iter<'a, HostKey, HostConfig>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<const N: usize> From<[(HostKey, HostConfig); N]> for HostConfigs {
    fn from(arr: [(HostKey, HostConfig); N]) -> Self {
        HostConfigs(HashMap::<HostKey, HostConfig>::from_iter(arr))
    }
}

/// Configuration for a specific host's rate limiting behavior
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostConfig {
    /// Maximum concurrent requests allowed to this host
    pub concurrency: Option<usize>,

    /// Minimum interval between requests to this host
    #[serde(default, with = "humantime_serde")]
    pub request_interval: Option<Duration>,

    /// Custom headers to send with requests to this host
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_headers")]
    #[serde(serialize_with = "serialize_headers")]
    pub headers: HeaderMap,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            concurrency: None,
            request_interval: None,
            headers: HeaderMap::new(),
        }
    }
}

impl HostConfig {
    /// Get the effective maximum concurrency, falling back to the global default
    #[must_use]
    pub fn effective_concurrency(&self, global_config: &RateLimitConfig) -> usize {
        self.concurrency.unwrap_or(global_config.concurrency)
    }

    /// Get the effective request interval, falling back to the global default
    #[must_use]
    pub fn effective_request_interval(&self, global_config: &RateLimitConfig) -> Duration {
        self.request_interval
            .unwrap_or(global_config.request_interval)
    }

    /// Merge `self` with `other`, where `self` takes precedence.
    ///
    /// Scalar fields (`concurrency`, `request_interval`) use `self`'s value if
    /// set, otherwise fall back to `other`'s.
    ///
    /// Headers are merged per key: a header key present in `self` fully
    /// replaces the same key in `other` (rather than appending), while header
    /// keys only present in `other` are kept.
    #[must_use]
    pub(crate) fn merge(self, other: Self) -> Self {
        // Start from the lower-precedence headers, then let `self`'s headers
        // override on a per-key basis.
        let mut headers = other.headers;
        for key in self.headers.keys() {
            headers.remove(key);
        }
        for (key, value) in self.headers {
            if let Some(key) = key {
                headers.append(key, value);
            }
        }

        Self {
            concurrency: self.concurrency.or(other.concurrency),
            request_interval: self.request_interval.or(other.request_interval),
            headers,
        }
    }
}

/// Custom deserializer for headers from TOML config format
fn deserialize_headers<'de, D>(deserializer: D) -> Result<HeaderMap, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let map = HashMap::<String, String>::deserialize(deserializer)?;
    let mut header_map = HeaderMap::new();

    for (name, value) in map {
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|e| serde::de::Error::custom(format!("Invalid header name '{name}': {e}")))?;
        let header_value = HeaderValue::from_str(&value).map_err(|e| {
            serde::de::Error::custom(format!("Invalid header value '{value}': {e}"))
        })?;
        header_map.insert(header_name, header_value);
    }

    Ok(header_map)
}

/// Custom serializer for headers to TOML config format
fn serialize_headers<S>(headers: &HeaderMap, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let map: HashMap<String, String> = headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_str().unwrap_or("").to_string()))
        .collect();
    map.serialize(serializer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_defaults_present() {
        let defaults = HostConfigs::builtin_defaults();

        let gnu = defaults
            .get(&HostKey::from("www.gnu.org"))
            .expect("www.gnu.org default should exist");
        assert_eq!(
            gnu.headers.get("user-agent").map(|v| v.to_str().unwrap()),
            Some("Mozilla/5.0")
        );

        let redhat = defaults
            .get(&HostKey::from("developers.redhat.com"))
            .expect("developers.redhat.com default should exist");
        assert_eq!(
            redhat
                .headers
                .get("user-agent")
                .map(|v| v.to_str().unwrap()),
            Some("curl/8.4.0")
        );
    }

    #[test]
    fn test_merge_headers_override_by_key() {
        // The higher-precedence side (`self`) replaces same-key headers from
        // the lower-precedence side (`other`), while disjoint keys are kept.
        let mut high = HeaderMap::new();
        high.insert("user-agent", "high".parse().unwrap());
        high.insert("x-only-high", "1".parse().unwrap());
        let high = HostConfig {
            headers: high,
            ..HostConfig::default()
        };

        let mut low = HeaderMap::new();
        low.insert("user-agent", "low".parse().unwrap());
        low.insert("x-only-low", "2".parse().unwrap());
        let low = HostConfig {
            headers: low,
            ..HostConfig::default()
        };

        let merged = high.merge(low);

        // Same-key header: high wins, low's value is dropped (not appended).
        let user_agents: Vec<_> = merged.headers.get_all("user-agent").iter().collect();
        assert_eq!(user_agents.len(), 1);
        assert_eq!(user_agents[0], "high");

        // Disjoint keys from both sides are preserved.
        assert_eq!(merged.headers.get("x-only-high").unwrap(), "1");
        assert_eq!(merged.headers.get("x-only-low").unwrap(), "2");
    }

    #[test]
    fn test_merge_user_overrides_builtin_default() {
        let key = HostKey::from("www.gnu.org");

        let mut user_headers = HeaderMap::new();
        user_headers.insert("user-agent", "my-agent".parse().unwrap());
        let user = HostConfigs::from([(
            key.clone(),
            HostConfig {
                concurrency: Some(3),
                headers: user_headers,
                ..HostConfig::default()
            },
        )]);

        // User config takes precedence over the built-in defaults.
        let merged = user.merge(HostConfigs::builtin_defaults());
        let config = merged.get(&key).unwrap();

        assert_eq!(config.concurrency, Some(3));
        let user_agents: Vec<_> = config.headers.get_all("user-agent").iter().collect();
        assert_eq!(user_agents.len(), 1);
        assert_eq!(user_agents[0], "my-agent");
    }

    #[test]
    fn test_default_rate_limit_config() {
        let config = RateLimitConfig::default();
        assert_eq!(config.concurrency, 10);
        assert_eq!(config.request_interval, Duration::from_millis(50));
    }

    #[test]
    fn test_host_config_effective_values() {
        let global_config = RateLimitConfig::default();

        // Test with no overrides
        let host_config = HostConfig::default();
        assert_eq!(host_config.effective_concurrency(&global_config), 10);
        assert_eq!(
            host_config.effective_request_interval(&global_config),
            Duration::from_millis(50)
        );

        // Test with overrides
        let host_config = HostConfig {
            concurrency: Some(5),
            request_interval: Some(Duration::from_millis(500)),
            headers: HeaderMap::new(),
        };
        assert_eq!(host_config.effective_concurrency(&global_config), 5);
        assert_eq!(
            host_config.effective_request_interval(&global_config),
            Duration::from_millis(500)
        );
    }

    #[test]
    fn test_config_serialization() {
        let config = RateLimitConfig {
            concurrency: 15,
            request_interval: Duration::from_millis(200),
        };

        let toml = toml::to_string(&config).unwrap();
        let deserialized: RateLimitConfig = toml::from_str(&toml).unwrap();

        assert_eq!(config.concurrency, deserialized.concurrency);
        assert_eq!(config.request_interval, deserialized.request_interval);
    }

    #[test]
    fn test_headers_serialization() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "Bearer token123".parse().unwrap());
        headers.insert("User-Agent", "test-agent".parse().unwrap());

        let host_config = HostConfig {
            concurrency: Some(5),
            request_interval: Some(Duration::from_millis(500)),
            headers,
        };

        let toml = toml::to_string(&host_config).unwrap();
        let deserialized: HostConfig = toml::from_str(&toml).unwrap();

        assert_eq!(deserialized.concurrency, Some(5));
        assert_eq!(
            deserialized.request_interval,
            Some(Duration::from_millis(500))
        );
        assert_eq!(deserialized.headers.len(), 2);
        assert!(deserialized.headers.contains_key("authorization"));
        assert!(deserialized.headers.contains_key("user-agent"));
    }
}
