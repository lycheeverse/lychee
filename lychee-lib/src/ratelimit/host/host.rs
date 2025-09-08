use dashmap::DashMap;
use governor::{
    Quota, RateLimiter,
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
};
use reqwest::{Client as ReqwestClient, Request, Response, redirect};
use reqwest_cookie_store::CookieStoreMutex;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

use super::key::HostKey;
use super::stats::HostStats;
use crate::ratelimit::{HostConfig, RateLimitConfig, RateLimitError};
use crate::{CacheStatus, Status, Uri};

/// Cache value for per-host caching
#[derive(Debug, Clone)]
struct HostCacheValue {
    status: CacheStatus,
    timestamp: Instant,
}

impl From<&Status> for HostCacheValue {
    fn from(status: &Status) -> Self {
        Self {
            status: status.into(),
            timestamp: Instant::now(),
        }
    }
}

/// Per-host cache for storing request results
type HostCache = DashMap<Uri, HostCacheValue>;

/// Represents a single host with its own rate limiting, concurrency control,
/// HTTP client configuration, and request cache.
///
/// Each host maintains:
/// - A token bucket rate limiter using governor
/// - A semaphore for concurrency control
/// - A dedicated HTTP client with host-specific headers and cookies
/// - Statistics tracking for adaptive behavior
/// - A per-host cache to prevent duplicate requests
#[derive(Debug)]
pub struct Host {
    /// The hostname this instance manages
    pub key: HostKey,

    /// Rate limiter using token bucket algorithm
    rate_limiter: RateLimiter<NotKeyed, InMemoryState, DefaultClock>,

    /// Controls maximum concurrent requests to this host
    semaphore: Arc<Semaphore>,

    /// HTTP client configured for this specific host
    client: ReqwestClient,

    /// Cookie jar for maintaining session state (per-host)
    #[allow(dead_code)]
    cookie_jar: Arc<CookieStoreMutex>,

    /// Request statistics and adaptive behavior tracking
    stats: Arc<Mutex<HostStats>>,

    /// Current backoff duration for adaptive rate limiting
    backoff_duration: Arc<Mutex<Duration>>,

    /// Per-host cache to prevent duplicate requests
    cache: HostCache,

    /// Maximum age for cached entries (in seconds)
    cache_max_age: u64,
}

impl Host {
    /// Create a new Host instance for the given hostname
    ///
    /// # Arguments
    ///
    /// * `key` - The hostname this host will manage
    /// * `host_config` - Host-specific configuration
    /// * `global_config` - Global defaults to fall back to
    /// * `cache_max_age` - Maximum age for cached entries in seconds (0 to disable caching)
    /// * `shared_cookie_jar` - Optional shared cookie jar to use instead of creating per-host jar
    /// * `global_headers` - Global headers to be applied to all requests (User-Agent, custom headers, etc.)
    /// * `max_redirects` - Maximum number of redirects to follow
    /// * `timeout` - Request timeout
    /// * `allow_insecure` - Whether to allow insecure certificates
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be configured properly
    ///
    /// # Panics
    ///
    /// Panics if the burst size cannot be set to 1 (should never happen)
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        key: HostKey,
        host_config: &HostConfig,
        global_config: &RateLimitConfig,
        cache_max_age: u64,
        shared_cookie_jar: Option<Arc<CookieStoreMutex>>,
        global_headers: &http::HeaderMap,
        max_redirects: usize,
        timeout: Option<Duration>,
        allow_insecure: bool,
    ) -> Result<Self, RateLimitError> {
        // Configure rate limiter with effective request interval
        let interval = host_config.effective_request_interval(global_config);
        let quota = Quota::with_period(interval)
            .ok_or_else(|| RateLimitError::HeaderParseError {
                host: key.to_string(),
                reason: "Invalid rate limit interval".to_string(),
            })?
            .allow_burst(std::num::NonZeroU32::new(1).unwrap());

        let rate_limiter = RateLimiter::direct(quota);

        // Create semaphore for concurrency control
        let max_concurrent = host_config.effective_max_concurrent(global_config);
        let semaphore = Arc::new(Semaphore::new(max_concurrent));

        // Use shared cookie jar if provided, otherwise create per-host one
        let cookie_jar = shared_cookie_jar.unwrap_or_else(|| Arc::new(CookieStoreMutex::default()));

        // Combine global headers with host-specific headers
        let mut combined_headers = global_headers.clone();
        for (name, value) in &host_config.headers {
            combined_headers.insert(name, value.clone());
        }

        // Create custom redirect policy matching main client behavior
        let redirect_policy = redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() > max_redirects {
                attempt.error("too many redirects")
            } else {
                log::debug!("Redirecting to {}", attempt.url());
                attempt.follow()
            }
        });

        // Build HTTP client with proper configuration
        let mut builder = ReqwestClient::builder()
            .cookie_provider(cookie_jar.clone())
            .default_headers(combined_headers)
            .gzip(true)
            .danger_accept_invalid_certs(allow_insecure)
            .connect_timeout(Duration::from_secs(10)) // CONNECT_TIMEOUT constant
            .tcp_keepalive(Duration::from_secs(60)) // TCP_KEEPALIVE constant
            .redirect(redirect_policy);

        if let Some(timeout) = timeout {
            builder = builder.timeout(timeout);
        }

        let client = builder
            .build()
            .map_err(|e| RateLimitError::ClientConfigError {
                host: key.to_string(),
                source: e,
            })?;

        Ok(Host {
            key,
            rate_limiter,
            semaphore,
            client,
            cookie_jar,
            stats: Arc::new(Mutex::new(HostStats::default())),
            backoff_duration: Arc::new(Mutex::new(Duration::from_millis(0))),
            cache: DashMap::new(),
            cache_max_age,
        })
    }

    /// Check if a URI is cached and return the cached status if valid
    ///
    /// # Panics
    ///
    /// Panics if the statistics mutex is poisoned
    pub fn get_cached_status(&self, uri: &Uri) -> Option<CacheStatus> {
        if self.cache_max_age == 0 {
            // Track cache miss when caching is disabled
            self.stats.lock().unwrap().record_cache_miss();
            return None; // Caching disabled
        }

        if let Some(entry) = self.cache.get(uri) {
            let age = entry.timestamp.elapsed().as_secs();
            if age <= self.cache_max_age {
                // Cache hit
                self.stats.lock().unwrap().record_cache_hit();
                return Some(entry.status);
            }
            // Cache entry expired, remove it
            drop(entry);
            self.cache.remove(uri);
        }
        // Cache miss
        self.stats.lock().unwrap().record_cache_miss();
        None
    }

    /// Cache a request result
    pub fn cache_result(&self, uri: &Uri, status: &Status) {
        if self.cache_max_age > 0 {
            let cache_value = HostCacheValue::from(status);
            self.cache.insert(uri.clone(), cache_value);
        }
    }

    /// Execute a request with rate limiting, concurrency control, and caching
    ///
    /// This method:
    /// 1. Checks the per-host cache for existing results
    /// 2. If not cached, acquires a semaphore permit for concurrency control
    /// 3. Waits for rate limiter permission
    /// 4. Applies adaptive backoff if needed
    /// 5. Executes the request
    /// 6. Updates statistics based on response
    /// 7. Parses rate limit headers to adjust future behavior
    /// 8. Caches the result for future use
    ///
    /// # Arguments
    ///
    /// * `request` - The HTTP request to execute
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or rate limiting is exceeded
    ///
    /// # Panics
    ///
    /// Panics if the statistics mutex is poisoned
    pub async fn execute_request(&self, request: Request) -> Result<Response, RateLimitError> {
        let uri = Uri::from(request.url().clone());

        // Note: Cache checking is handled at the HostPool level
        // This method focuses on executing the actual HTTP request

        // Acquire semaphore permit for concurrency control
        let _permit =
            self.semaphore
                .acquire()
                .await
                .map_err(|_| RateLimitError::RateLimitExceeded {
                    host: self.key.to_string(),
                    message: "Semaphore acquisition cancelled".to_string(),
                })?;

        // Apply adaptive backoff if needed
        let backoff_duration = {
            let backoff = self.backoff_duration.lock().unwrap();
            *backoff
        };
        if !backoff_duration.is_zero() {
            log::debug!(
                "Host {} applying backoff delay of {}ms due to previous rate limiting or errors",
                self.key,
                backoff_duration.as_millis()
            );
            tokio::time::sleep(backoff_duration).await;
        }

        // Wait for rate limiter permission
        self.rate_limiter.until_ready().await;

        // Execute the request and track timing
        let start_time = Instant::now();
        let response = match self.client.execute(request).await {
            Ok(response) => response,
            Err(e) => {
                // Wrap network/HTTP errors to preserve the original error
                return Err(RateLimitError::NetworkError {
                    host: self.key.to_string(),
                    source: e,
                });
            }
        };
        let request_time = start_time.elapsed();

        // Update statistics based on response
        let status_code = response.status().as_u16();
        self.update_stats_and_backoff(status_code, request_time);

        // Parse rate limit headers to adjust behavior
        self.parse_rate_limit_headers(&response);

        // Cache the result
        let status = Status::Ok(response.status());
        self.cache_result(&uri, &status);

        Ok(response)
    }

    /// Update internal statistics and backoff based on the response
    fn update_stats_and_backoff(&self, status_code: u16, request_time: Duration) {
        // Update statistics
        {
            let mut stats = self.stats.lock().unwrap();
            stats.record_response(status_code, request_time);
        }

        // Update backoff duration based on response
        {
            let mut backoff = self.backoff_duration.lock().unwrap();
            match status_code {
                200..=299 => {
                    // Reset backoff on success
                    *backoff = Duration::from_millis(0);
                }
                429 => {
                    // Exponential backoff on rate limit, capped at 30 seconds
                    let new_backoff = std::cmp::min(
                        if backoff.is_zero() {
                            Duration::from_millis(500)
                        } else {
                            *backoff * 2
                        },
                        Duration::from_secs(30),
                    );
                    log::debug!(
                        "Host {} hit rate limit (429), increasing backoff from {}ms to {}ms",
                        self.key,
                        backoff.as_millis(),
                        new_backoff.as_millis()
                    );
                    *backoff = new_backoff;
                }
                500..=599 => {
                    // Moderate backoff increase on server errors, capped at 10 seconds
                    *backoff = std::cmp::min(
                        *backoff + Duration::from_millis(200),
                        Duration::from_secs(10),
                    );
                }
                _ => {} // No backoff change for other status codes
            }
        }
    }

    /// Parse rate limit headers from response and adjust behavior
    fn parse_rate_limit_headers(&self, response: &Response) {
        // Manual parsing of common rate limit headers
        // We implement basic parsing here for the most common headers (X-RateLimit-*, Retry-After)
        // rather than using the rate-limits crate to keep dependencies minimal

        let headers = response.headers();

        // Try common rate limit header patterns
        let remaining = Self::parse_header_value(
            headers,
            &[
                "x-ratelimit-remaining",
                "x-rate-limit-remaining",
                "ratelimit-remaining",
            ],
        );

        let limit = Self::parse_header_value(
            headers,
            &["x-ratelimit-limit", "x-rate-limit-limit", "ratelimit-limit"],
        );

        if let (Some(remaining), Some(limit)) = (remaining, limit) {
            if limit > 0 {
                #[allow(clippy::cast_precision_loss)]
                let usage_ratio = (limit - remaining) as f64 / limit as f64;

                // If we've used more than 80% of our quota, apply preventive backoff
                if usage_ratio > 0.8 {
                    let mut backoff = self.backoff_duration.lock().unwrap();
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let preventive_backoff =
                        Duration::from_millis((200.0 * (usage_ratio - 0.8) / 0.2) as u64);
                    *backoff = std::cmp::max(*backoff, preventive_backoff);
                }
            }
        }

        // Check for Retry-After header (in seconds)
        if let Some(retry_after_value) = headers.get("retry-after") {
            if let Ok(retry_after_str) = retry_after_value.to_str() {
                if let Ok(retry_seconds) = retry_after_str.parse::<u64>() {
                    let mut backoff = self.backoff_duration.lock().unwrap();
                    let retry_duration = Duration::from_secs(retry_seconds);
                    // Cap retry-after to reasonable limits
                    if retry_duration <= Duration::from_secs(3600) {
                        *backoff = std::cmp::max(*backoff, retry_duration);
                    }
                }
            }
        }
    }

    /// Helper method to parse numeric header values from common rate limit headers
    fn parse_header_value(headers: &http::HeaderMap, header_names: &[&str]) -> Option<usize> {
        for header_name in header_names {
            if let Some(value) = headers.get(*header_name) {
                if let Ok(value_str) = value.to_str() {
                    if let Ok(number) = value_str.parse::<usize>() {
                        return Some(number);
                    }
                }
            }
        }
        None
    }

    /// Get host statistics
    ///
    /// # Panics
    ///
    /// Panics if the statistics mutex is poisoned
    pub fn stats(&self) -> HostStats {
        self.stats.lock().unwrap().clone()
    }

    /// Record a cache hit from the persistent disk cache
    ///
    /// # Panics
    ///
    /// Panics if the statistics mutex is poisoned
    pub fn record_persistent_cache_hit(&self) {
        self.stats.lock().unwrap().record_cache_hit();
    }

    /// Record a cache miss from the persistent disk cache
    ///
    /// # Panics
    ///
    /// Panics if the statistics mutex is poisoned
    pub fn record_persistent_cache_miss(&self) {
        self.stats.lock().unwrap().record_cache_miss();
    }

    /// Get the current number of available permits (concurrent request slots)
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Get the current cache size (number of cached entries)
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }

    /// Clear expired entries from the cache
    pub fn cleanup_cache(&self) {
        if self.cache_max_age == 0 {
            return;
        }

        self.cache
            .retain(|_, value| value.timestamp.elapsed().as_secs() <= self.cache_max_age);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ratelimit::{HostConfig, RateLimitConfig};
    use std::time::Duration;

    #[tokio::test]
    async fn test_host_creation() {
        let key = HostKey::from("example.com");
        let host_config = HostConfig::default();
        let global_config = RateLimitConfig::default();

        let host = Host::new(
            key.clone(),
            &host_config,
            &global_config,
            3600,
            None,
            &http::HeaderMap::new(),
            5,
            Some(std::time::Duration::from_secs(20)),
            false,
        )
        .unwrap();

        assert_eq!(host.key, key);
        assert_eq!(host.available_permits(), 10); // Default concurrency
        assert!((host.stats().success_rate() - 1.0).abs() < f64::EPSILON);
        assert_eq!(host.cache_size(), 0);
    }

    #[test]
    fn test_cache_expiration() {
        let key = HostKey::from("example.com");
        let host_config = HostConfig::default();
        let global_config = RateLimitConfig::default();

        let host = Host::new(
            key,
            &host_config,
            &global_config,
            1,
            None,
            &http::HeaderMap::new(),
            5,
            Some(std::time::Duration::from_secs(20)),
            false,
        )
        .unwrap(); // 1 second cache

        let uri = Uri::from("https://example.com/test".parse::<reqwest::Url>().unwrap());
        let status = Status::Ok(http::StatusCode::OK);

        // Cache the result
        host.cache_result(&uri, &status);
        assert_eq!(host.cache_size(), 1);

        // Should be in cache immediately
        assert!(host.get_cached_status(&uri).is_some());

        // Wait for expiration and cleanup
        std::thread::sleep(Duration::from_secs(2));
        host.cleanup_cache();

        // Should be expired now
        assert!(host.get_cached_status(&uri).is_none());
    }
}
