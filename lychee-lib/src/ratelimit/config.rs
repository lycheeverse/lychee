use http::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::ratelimit::{HostKey, RequestInterval};

/// Global rate limiting configuration that applies as defaults to all hosts
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Default maximum concurrent requests per host
    #[serde(default = "default_host_concurrency")]
    pub host_concurrency: usize,

    /// Default minimum interval between requests to the same host
    pub request_interval: RequestInterval,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            host_concurrency: default_host_concurrency(),
            request_interval: RequestInterval::default(),
        }
    }
}

impl RateLimitConfig {
    /// Create a `RateLimitConfig` from CLI options, using defaults for missing values
    #[must_use]
    pub fn from_options(
        host_concurrency: Option<usize>,
        request_interval: Option<RequestInterval>,
    ) -> Self {
        Self {
            host_concurrency: host_concurrency.unwrap_or(DEFAULT_HOST_CONCURRENCY),
            request_interval: request_interval.unwrap_or_default(),
        }
    }
}

/// Per-host configuration overrides
pub type HostConfigs = HashMap<HostKey, HostConfig>;

/// Configuration for a specific host's rate limiting behavior
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostConfig {
    /// Maximum concurrent requests allowed to this host
    pub max_concurrent: Option<usize>,

    /// Minimum interval between requests to this host
    pub request_interval: Option<RequestInterval>,

    /// Custom headers to send with requests to this host
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_headers")]
    #[serde(serialize_with = "serialize_headers")]
    pub headers: HeaderMap,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            max_concurrent: None,
            request_interval: None,
            headers: HeaderMap::new(),
        }
    }
}

impl HostConfig {
    /// Get the effective max concurrency, falling back to the global default
    #[must_use]
    pub fn effective_max_concurrent(&self, global_config: &RateLimitConfig) -> usize {
        self.max_concurrent
            .unwrap_or(global_config.host_concurrency)
    }

    /// Get the effective request interval, falling back to the global default
    #[must_use]
    pub fn effective_request_interval(&self, global_config: &RateLimitConfig) -> RequestInterval {
        self.request_interval
            .unwrap_or(global_config.request_interval)
    }
}

/// Default number of concurrent requests per host
const DEFAULT_HOST_CONCURRENCY: usize = 10;

/// Default number of concurrent requests per host
const fn default_host_concurrency() -> usize {
    DEFAULT_HOST_CONCURRENCY
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
    use std::time::Duration;

    use governor::Quota;

    use super::*;

    #[test]
    fn test_host_config_effective_values() {
        let global_config = RateLimitConfig::default();

        // Test with no overrides
        let host_config = HostConfig::default();
        assert_eq!(host_config.effective_max_concurrent(&global_config), 10);
        assert_eq!(
            host_config.effective_request_interval(&global_config),
            RequestInterval::default(),
        );

        // Test with overrides
        let host_config = HostConfig {
            max_concurrent: Some(5),
            request_interval: Some("500ms".parse().unwrap()),
            headers: HeaderMap::new(),
        };
        assert_eq!(host_config.effective_max_concurrent(&global_config), 5);
        assert_eq!(
            host_config
                .effective_request_interval(&global_config)
                .into_inner(),
            Quota::with_period(Duration::from_millis(500)).unwrap()
        );
    }

    #[test]
    fn test_config_serialization() {
        let config = RateLimitConfig {
            host_concurrency: 15,
            request_interval: "200ms".parse().unwrap(),
        };

        let toml = toml::to_string(&config).unwrap();
        let deserialized: RateLimitConfig = toml::from_str(&toml).unwrap();

        assert_eq!(config.host_concurrency, deserialized.host_concurrency);
        assert_eq!(config.request_interval, deserialized.request_interval);
    }

    #[test]
    fn test_headers_serialization() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "Bearer token123".parse().unwrap());
        headers.insert("User-Agent", "test-agent".parse().unwrap());

        let host_config = HostConfig {
            max_concurrent: Some(5),
            request_interval: Some("500ms".parse().unwrap()),
            headers,
        };

        let toml = toml::to_string(&host_config).unwrap();
        let deserialized: HostConfig = toml::from_str(&dbg!(toml)).unwrap();

        assert_eq!(deserialized.max_concurrent, Some(5));
        assert_eq!(
            deserialized.request_interval,
            Some("500ms".parse().unwrap())
        );
        assert_eq!(deserialized.headers.len(), 2);
        assert!(deserialized.headers.contains_key("authorization"));
        assert!(deserialized.headers.contains_key("user-agent"));
    }
}
