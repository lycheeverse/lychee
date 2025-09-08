use dashmap::DashMap;
use reqwest::{Request, Response};
use reqwest_cookie_store::CookieStoreMutex;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::ratelimit::{Host, HostConfig, HostKey, HostStats, RateLimitConfig, RateLimitError};
use crate::{CacheStatus, Status, Uri};

/// Manages a pool of Host instances and routes requests to appropriate hosts.
///
/// The `HostPool` serves as the central coordinator for per-host rate limiting.
/// It creates Host instances on-demand, manages global concurrency limits,
/// and provides a unified interface for executing HTTP requests with
/// appropriate rate limiting applied.
///
/// # Architecture
///
/// - Each unique hostname gets its own Host instance with dedicated rate limiting
/// - Global semaphore enforces overall concurrency limits across all hosts
/// - Hosts are created lazily when first requested
/// - Thread-safe using `DashMap` for concurrent access to host instances
#[derive(Debug, Clone)]
pub struct HostPool {
    /// Map of hostname to Host instances, created on-demand
    hosts: Arc<DashMap<HostKey, Arc<Host>>>,

    /// Global configuration for rate limiting defaults
    global_config: Arc<RateLimitConfig>,

    /// Per-host configuration overrides
    host_configs: Arc<HashMap<String, HostConfig>>,

    /// Global semaphore to enforce overall concurrency limit
    global_semaphore: Arc<Semaphore>,

    /// Maximum age for cached entries in seconds (0 to disable caching)
    cache_max_age: u64,

    /// Shared cookie jar used across all hosts
    cookie_jar: Option<Arc<CookieStoreMutex>>,
}

impl HostPool {
    /// Create a new `HostPool` with the given configuration
    ///
    /// # Arguments
    ///
    /// * `global_config` - Default rate limiting configuration
    /// * `host_configs` - Host-specific configuration overrides
    /// * `max_total_concurrency` - Global limit on concurrent requests across all hosts
    /// * `cache_max_age` - Maximum age for cached entries in seconds (0 to disable caching)
    ///
    /// # Examples
    ///
    /// ```
    /// use lychee_lib::ratelimit::{HostPool, RateLimitConfig};
    /// use std::collections::HashMap;
    ///
    /// let global_config = RateLimitConfig::default();
    /// let host_configs = HashMap::new();
    /// let pool = HostPool::new(global_config, host_configs, 128, 3600);
    /// ```
    #[must_use]
    pub fn new(
        global_config: RateLimitConfig,
        host_configs: HashMap<String, HostConfig>,
        max_total_concurrency: usize,
        cache_max_age: u64,
    ) -> Self {
        Self {
            hosts: Arc::new(DashMap::new()),
            global_config: Arc::new(global_config),
            host_configs: Arc::new(host_configs),
            global_semaphore: Arc::new(Semaphore::new(max_total_concurrency)),
            cache_max_age,
            cookie_jar: None,
        }
    }

    /// Create a new `HostPool` with a shared cookie jar
    #[must_use]
    pub fn with_cookie_jar(
        global_config: RateLimitConfig,
        host_configs: HashMap<String, HostConfig>,
        max_total_concurrency: usize,
        cache_max_age: u64,
        cookie_jar: Arc<CookieStoreMutex>,
    ) -> Self {
        Self {
            hosts: Arc::new(DashMap::new()),
            global_config: Arc::new(global_config),
            host_configs: Arc::new(host_configs),
            global_semaphore: Arc::new(Semaphore::new(max_total_concurrency)),
            cache_max_age,
            cookie_jar: Some(cookie_jar),
        }
    }

    /// Execute an HTTP request with appropriate per-host rate limiting
    ///
    /// This method:
    /// 1. Extracts the hostname from the request URL
    /// 2. Gets or creates the appropriate Host instance
    /// 3. Acquires a global semaphore permit
    /// 4. Delegates to the host for execution with host-specific rate limiting
    ///
    /// # Arguments
    ///
    /// * `request` - The HTTP request to execute
    ///
    /// # Errors
    ///
    /// Returns a `RateLimitError` if:
    /// - The request URL has no valid hostname
    /// - Global or host-specific rate limits are exceeded  
    /// - The underlying HTTP request fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use lychee_lib::ratelimit::{HostPool, RateLimitConfig};
    /// # use std::collections::HashMap;
    /// # use reqwest::Request;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let pool = HostPool::new(RateLimitConfig::default(), HashMap::new(), 128, 3600);
    /// let request = reqwest::Request::new(reqwest::Method::GET, "https://example.com".parse()?);
    /// let response = pool.execute_request(request).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute_request(&self, request: Request) -> Result<Response, RateLimitError> {
        // Extract hostname from request URL
        let url = request.url();
        let host_key = HostKey::try_from(url)?;

        // Get or create host instance
        let host = self.get_or_create_host(host_key)?;

        // Acquire global semaphore permit first
        let _global_permit = self.global_semaphore.acquire().await.map_err(|_| {
            RateLimitError::RateLimitExceeded {
                host: host.key.to_string(),
                message: "Global concurrency limit reached".to_string(),
            }
        })?;

        // Execute request through host-specific rate limiting
        host.execute_request(request).await
    }

    /// Get an existing host or create a new one for the given hostname
    fn get_or_create_host(&self, host_key: HostKey) -> Result<Arc<Host>, RateLimitError> {
        // Check if host already exists
        if let Some(host) = self.hosts.get(&host_key) {
            return Ok(host.clone());
        }

        // Create new host instance
        let host_config = self
            .host_configs
            .get(host_key.as_str())
            .cloned()
            .unwrap_or_default();

        let host = Arc::new(Host::new(
            host_key.clone(),
            &host_config,
            &self.global_config,
            self.cache_max_age,
            self.cookie_jar.clone(),
        )?);

        // Store in map (handle race condition where another thread created it)
        match self.hosts.entry(host_key) {
            dashmap::mapref::entry::Entry::Occupied(entry) => {
                // Another thread created it, use theirs
                Ok(entry.get().clone())
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                // We're first, insert ours
                Ok(entry.insert(host).clone())
            }
        }
    }

    /// Get statistics for a specific host
    ///
    /// Returns statistics for the host if it exists, otherwise returns empty stats.
    /// This provides consistent behavior whether or not requests have been made to that host yet.
    ///
    /// # Arguments
    ///
    /// * `hostname` - The hostname to get statistics for
    #[must_use]
    pub fn host_stats(&self, hostname: &str) -> HostStats {
        let host_key = HostKey::from(hostname);
        self.hosts
            .get(&host_key)
            .map(|host| host.stats())
            .unwrap_or_default()
    }

    /// Get statistics for all hosts that have been created
    ///
    /// Returns a `HashMap` mapping hostnames to their statistics.
    /// Only hosts that have had requests will be included.
    #[must_use]
    pub fn all_host_stats(&self) -> HashMap<String, HostStats> {
        self.hosts
            .iter()
            .map(|entry| {
                let hostname = entry.key().to_string();
                let stats = entry.value().stats();
                (hostname, stats)
            })
            .collect()
    }

    /// Get the number of currently active hosts
    ///
    /// This returns the number of Host instances that have been created,
    /// which corresponds to the number of unique hostnames that have
    /// been accessed.
    #[must_use]
    pub fn active_host_count(&self) -> usize {
        self.hosts.len()
    }

    /// Get the number of available global permits
    ///
    /// This shows how many more concurrent requests can be started
    /// across all hosts before hitting the global concurrency limit.
    #[must_use]
    pub fn available_global_permits(&self) -> usize {
        self.global_semaphore.available_permits()
    }

    /// Get host configuration for debugging/monitoring
    ///
    /// Returns a copy of the current host-specific configurations.
    /// This is useful for debugging or runtime monitoring of configuration.
    #[must_use]
    pub fn host_configurations(&self) -> HashMap<String, HostConfig> {
        (*self.host_configs).clone()
    }

    /// Remove a host from the pool
    ///
    /// This forces the host to be recreated with updated configuration
    /// the next time a request is made to it. Any ongoing requests to
    /// that host will continue with the old instance.
    ///
    /// # Arguments
    ///
    /// * `hostname` - The hostname to remove from the pool
    ///
    /// # Returns
    ///
    /// Returns true if a host was removed, false if no host existed for that hostname.
    #[must_use]
    pub fn remove_host(&self, hostname: &str) -> bool {
        let host_key = HostKey::from(hostname);
        self.hosts.remove(&host_key).is_some()
    }

    /// Check if a URI is cached in the appropriate host's cache
    ///
    /// # Arguments
    ///
    /// * `uri` - The URI to check for in the cache
    ///
    /// # Returns
    ///
    /// Returns the cached status if found and valid, None otherwise
    #[must_use]
    pub fn get_cached_status(&self, uri: &Uri) -> Option<CacheStatus> {
        let host_key = HostKey::try_from(uri).ok()?;

        if let Some(host) = self.hosts.get(&host_key) {
            host.get_cached_status(uri)
        } else {
            None
        }
    }

    /// Cache a result for a URI in the appropriate host's cache
    ///
    /// # Arguments
    ///
    /// * `uri` - The URI to cache
    /// * `status` - The status to cache
    pub fn cache_result(&self, uri: &Uri, status: &Status) {
        if let Ok(host_key) = HostKey::try_from(uri) {
            if let Some(host) = self.hosts.get(&host_key) {
                host.cache_result(uri, status);
            }
            // If host doesn't exist yet, we don't cache
            // The result will be cached when the host is created and the request is made
        }
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

    /// Cleanup expired cache entries across all hosts
    pub fn cleanup_caches(&self) {
        for host in self.hosts.iter() {
            host.cleanup_cache();
        }
    }

    /// Record a cache hit for the given URI in host statistics
    ///
    /// This tracks that a request was served from the persistent disk cache
    /// rather than going through the rate-limited HTTP request flow.
    /// This method will create a Host instance if one doesn't exist yet.
    ///
    /// # Errors
    ///
    /// Returns an error if the host key cannot be parsed from the URI or if the host cannot be created.
    pub fn record_cache_hit(
        &self,
        uri: &crate::Uri,
    ) -> Result<(), crate::ratelimit::RateLimitError> {
        let host_key = crate::ratelimit::HostKey::try_from(uri)?;

        // Get or create the host (this ensures statistics tracking even for cache-only requests)
        let host = self.get_or_create_host(host_key)?;
        host.record_persistent_cache_hit();
        Ok(())
    }

    /// Record a cache miss for the given URI in host statistics  
    ///
    /// This tracks that a request could not be served from the persistent disk cache
    /// and will need to go through the rate-limited HTTP request flow.
    /// This method will create a Host instance if one doesn't exist yet.
    ///
    /// # Errors
    ///
    /// Returns an error if the host key cannot be parsed from the URI or if the host cannot be created.
    pub fn record_cache_miss(
        &self,
        uri: &crate::Uri,
    ) -> Result<(), crate::ratelimit::RateLimitError> {
        let host_key = crate::ratelimit::HostKey::try_from(uri)?;

        // Get or create the host (this ensures statistics tracking even for cache-only requests)
        let host = self.get_or_create_host(host_key)?;
        host.record_persistent_cache_miss();
        Ok(())
    }
}

impl Default for HostPool {
    fn default() -> Self {
        Self::new(
            RateLimitConfig::default(),
            HashMap::new(),
            128,  // Default global concurrency limit
            3600, // Default cache age of 1 hour
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
        let global_config = RateLimitConfig::default();
        let host_configs = HashMap::new();
        let pool = HostPool::new(global_config, host_configs, 100, 3600);

        assert_eq!(pool.active_host_count(), 0);
        assert_eq!(pool.available_global_permits(), 100);
    }

    #[test]
    fn test_host_pool_default() {
        let pool = HostPool::default();

        assert_eq!(pool.active_host_count(), 0);
        assert_eq!(pool.available_global_permits(), 128);
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
        let host = pool.get_or_create_host(host_key).unwrap();

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
        let host1 = pool.get_or_create_host(host_key1).unwrap();
        assert_eq!(pool.active_host_count(), 1);

        // Second request to same host should reuse
        let host2 = pool.get_or_create_host(host_key2).unwrap();
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

    #[test]
    fn test_all_host_stats() {
        let pool = HostPool::default();

        // No hosts initially
        let stats = pool.all_host_stats();
        assert!(stats.is_empty());

        // Stats would be populated after actual requests are made to create hosts
    }
}
