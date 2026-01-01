use dashmap::DashMap;
use http::Method;
use reqwest::{Client, Request};
use std::collections::HashMap;
use std::sync::Arc;

use crate::ratelimit::{
    CacheableResponse, Host, HostConfigs, HostKey, HostStats, HostStatsMap, RateLimitConfig,
};
use crate::types::Result;
use crate::{ErrorKind, Uri};

/// Keep track of host-specific [`reqwest::Client`]s
pub type ClientMap = HashMap<HostKey, reqwest::Client>;

/// Manages a pool of Host instances and routes requests to appropriate hosts.
///
/// The `HostPool` serves as the central coordinator for per-host rate limiting.
/// It creates host instances on-demand and provides a unified interface for
/// executing HTTP requests with appropriate rate limiting applied.
///
/// # Architecture
///
/// - Each unique hostname gets its own Host instance with dedicated rate limiting
/// - Hosts are created lazily when first requested
/// - Thread-safe using `DashMap` for concurrent access to host instances
#[derive(Debug)]
pub struct HostPool {
    /// Map of hostname to Host instances, created on-demand
    hosts: DashMap<HostKey, Arc<Host>>,

    /// Global configuration for rate limiting defaults
    global_config: RateLimitConfig,

    /// Per-host configuration overrides
    host_configs: HostConfigs,

    /// Fallback client for hosts without host-specific client
    default_client: Client,

    /// Host-specific clients
    client_map: ClientMap,
}

impl HostPool {
    /// Create a new `HostPool` with the given configuration
    #[must_use]
    pub fn new(
        global_config: RateLimitConfig,
        host_configs: HostConfigs,
        default_client: Client,
        client_map: ClientMap,
    ) -> Self {
        Self {
            hosts: DashMap::new(),
            global_config,
            host_configs,
            default_client,
            client_map,
        }
    }

    /// Try to execute a [`Request`] with appropriate per-host rate limiting.
    ///
    /// # Errors
    ///
    /// Fails if:
    /// - The request URL has no valid hostname
    /// - The underlying HTTP request fails
    pub(crate) async fn execute_request(&self, request: Request) -> Result<CacheableResponse> {
        let url = request.url();
        let host_key = HostKey::try_from(url)?;
        let host = self.get_or_create_host(host_key);
        host.execute_request(request).await
    }

    /// Try to build a [`Request`]
    ///
    /// # Errors
    ///
    /// Fails if:
    /// - The request URI has no valid hostname
    /// - The request fails to build
    pub fn build_request(&self, method: Method, uri: &Uri) -> Result<Request> {
        let host_key = HostKey::try_from(uri)?;
        let host = self.get_or_create_host(host_key);
        host.get_client()
            .request(method, uri.url.clone())
            .build()
            .map_err(ErrorKind::BuildRequestClient)
    }

    /// Get an existing host or create a new one for the given hostname
    fn get_or_create_host(&self, host_key: HostKey) -> Arc<Host> {
        self.hosts
            .entry(host_key.clone())
            .or_insert_with(|| {
                let host_config = self
                    .host_configs
                    .get(&host_key)
                    .cloned()
                    .unwrap_or_default();

                let client = self
                    .client_map
                    .get(&host_key)
                    .unwrap_or(&self.default_client)
                    .clone();

                Arc::new(Host::new(
                    host_key,
                    &host_config,
                    &self.global_config,
                    client,
                ))
            })
            .value()
            .clone()
    }

    /// Returns statistics for the host if it exists, otherwise returns empty stats.
    /// This provides consistent behavior whether or not requests have been made to that host yet.
    #[must_use]
    pub fn host_stats(&self, hostname: &str) -> HostStats {
        let host_key = HostKey::from(hostname);
        self.hosts
            .get(&host_key)
            .map(|host| host.stats())
            .unwrap_or_default()
    }

    /// Returns a `HashMap` mapping hostnames to their statistics.
    /// Only hosts that have had requests will be included.
    #[must_use]
    pub fn all_host_stats(&self) -> HostStatsMap {
        HostStatsMap::from(
            self.hosts
                .iter()
                .map(|entry| {
                    let hostname = entry.key().to_string();
                    let stats = entry.value().stats();
                    (hostname, stats)
                })
                .collect::<HashMap<_, _>>(),
        )
    }

    /// Get the number of host instances that have been created,
    /// which corresponds to the number of unique hostnames that have
    /// been accessed.
    #[must_use]
    pub fn active_host_count(&self) -> usize {
        self.hosts.len()
    }

    /// Get  a copy of the current host-specific configurations.
    /// This is useful for debugging or runtime monitoring of configuration.
    #[must_use]
    pub fn host_configurations(&self) -> HostConfigs {
        self.host_configs.clone()
    }

    /// Remove a host from the pool.
    ///
    /// This forces the host to be recreated with updated configuration
    /// the next time a request is made to it. Any ongoing requests to
    /// that host will continue with the old instance.
    ///
    /// # Returns
    ///
    /// Returns true if a host was removed, false if no host existed for that hostname.
    #[must_use]
    pub fn remove_host(&self, hostname: &str) -> bool {
        let host_key = HostKey::from(hostname);
        self.hosts.remove(&host_key).is_some()
    }

    /// Get cache statistics across all hosts
    #[must_use]
    pub fn cache_stats(&self) -> HashMap<String, (usize, f64)> {
        self.hosts
            .iter()
            .map(|entry| {
                let hostname = entry.key().to_string();
                let cache_size = entry.value().cache_size();
                let hit_rate = entry.value().stats().cache_hit_rate();
                (hostname, (cache_size, hit_rate))
            })
            .collect()
    }

    /// Record a cache hit for the given URI in host statistics.
    /// This tracks that a request was served from the persistent disk cache.
    /// Note that no equivalent function for tracking cache misses is exposed,
    /// since this is handled internally.
    pub fn record_persistent_cache_hit(&self, uri: &crate::Uri) {
        if !uri.is_file() && !uri.is_mail() {
            match crate::ratelimit::HostKey::try_from(uri) {
                Ok(key) => {
                    let host = self.get_or_create_host(key);
                    host.record_persistent_cache_hit();
                }
                Err(e) => {
                    log::debug!("Failed to record cache hit for {uri}: {e}");
                }
            }
        }
    }
}

impl Default for HostPool {
    fn default() -> Self {
        Self::new(
            RateLimitConfig::default(),
            HostConfigs::default(),
            Client::default(),
            HashMap::new(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ratelimit::RateLimitConfig;

    use url::Url;

    #[test]
    fn test_host_pool_creation() {
        let pool = HostPool::new(
            RateLimitConfig::default(),
            HostConfigs::default(),
            Client::default(),
            HashMap::new(),
        );

        assert_eq!(pool.active_host_count(), 0);
    }

    #[test]
    fn test_host_pool_default() {
        let pool = HostPool::default();
        assert_eq!(pool.active_host_count(), 0);
    }

    #[tokio::test]
    async fn test_host_creation_on_demand() {
        let pool = HostPool::default();
        let url: Url = "https://example.com/path".parse().unwrap();
        let host_key = HostKey::try_from(&url).unwrap();

        // No hosts initially
        assert_eq!(pool.active_host_count(), 0);
        assert_eq!(pool.host_stats("example.com").total_requests, 0);

        // Create host on demand
        let host = pool.get_or_create_host(host_key);

        // Now we have one host
        assert_eq!(pool.active_host_count(), 1);
        assert_eq!(pool.host_stats("example.com").total_requests, 0);
        assert_eq!(host.key.as_str(), "example.com");
    }

    #[tokio::test]
    async fn test_host_reuse() {
        let pool = HostPool::default();
        let url: Url = "https://example.com/path1".parse().unwrap();
        let host_key1 = HostKey::try_from(&url).unwrap();

        let url: Url = "https://example.com/path2".parse().unwrap();
        let host_key2 = HostKey::try_from(&url).unwrap();

        // Create host for first request
        let host1 = pool.get_or_create_host(host_key1);
        assert_eq!(pool.active_host_count(), 1);

        // Second request to same host should reuse
        let host2 = pool.get_or_create_host(host_key2);
        assert_eq!(pool.active_host_count(), 1);

        // Should be the same instance
        assert!(Arc::ptr_eq(&host1, &host2));
    }

    #[test]
    fn test_host_config_management() {
        let pool = HostPool::default();

        // Initially no host configurations
        let configs = pool.host_configurations();
        assert_eq!(configs.len(), 0);
    }

    #[test]
    fn test_host_removal() {
        let pool = HostPool::default();

        // Remove non-existent host
        assert!(!pool.remove_host("nonexistent.com"));

        // We can't easily test removal of existing hosts without making actual requests
        // due to the async nature of host creation, but the basic functionality works
    }
}
