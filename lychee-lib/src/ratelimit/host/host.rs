use dashmap::DashMap;
use governor::{
    RateLimiter,
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
};
use reqwest::{Client as ReqwestClient, Request, Response};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

use super::key::HostKey;
use super::stats::HostStats;
use crate::types::Result;
use crate::{CacheStatus, Status, Uri};
use crate::{
    ErrorKind,
    ratelimit::{HostConfig, RateLimitConfig},
};

/// Cache value for per-host caching
#[derive(Debug, Clone)]
struct HostCacheValue {
    status: CacheStatus,
}

impl From<&Status> for HostCacheValue {
    fn from(status: &Status) -> Self {
        Self {
            status: status.into(),
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
    semaphore: Semaphore,

    /// HTTP client configured for this specific host
    client: ReqwestClient,

    /// Request statistics and adaptive behavior tracking
    stats: Mutex<HostStats>,

    /// Current backoff duration for adaptive rate limiting
    backoff_duration: Mutex<Duration>,

    /// Per-host cache to prevent duplicate requests
    cache: HostCache,
}

impl Host {
    /// Create a new Host instance for the given hostname
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be configured properly
    ///
    /// # Panics
    ///
    /// Panics if the burst size cannot be set to 1 (should never happen)
    pub fn new(
        key: HostKey,
        host_config: &HostConfig,
        global_config: &RateLimitConfig,
        client: ReqwestClient,
    ) -> Result<Self> {
        let quota = host_config
            .effective_request_interval(global_config)
            .into_inner()
            .allow_burst(std::num::NonZeroU32::new(1).unwrap());

        let rate_limiter = RateLimiter::direct(quota);

        // Create semaphore for concurrency control
        let max_concurrent = host_config.effective_max_concurrent(global_config);
        let semaphore = Semaphore::new(max_concurrent);

        Ok(Host {
            key,
            rate_limiter,
            semaphore,
            client,
            stats: Mutex::new(HostStats::default()),
            backoff_duration: Mutex::new(Duration::from_millis(0)),
            cache: DashMap::new(),
        })
    }

    /// Check if a URI is cached and return the cached status if valid
    ///
    /// # Panics
    ///
    /// Panics if the statistics mutex is poisoned
    pub fn get_cached_status(&self, uri: &Uri) -> Option<CacheStatus> {
        if let Some(entry) = self.cache.get(uri) {
            // Cache hit
            self.stats.lock().unwrap().record_cache_hit();
            return Some(entry.status);
        }

        // Cache miss
        self.stats.lock().unwrap().record_cache_miss();
        None
    }

    /// Cache a request result
    pub fn cache_result(&self, uri: &Uri, status: &Status) {
        let cache_value = HostCacheValue::from(status);
        self.cache.insert(uri.clone(), cache_value);
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
    pub async fn execute_request(&self, request: Request) -> Result<Response> {
        let uri = Uri::from(request.url().clone());

        // Note: Cache checking is handled at the HostPool level
        // This method focuses on executing the actual HTTP request

        // Acquire semaphore permit for concurrency control
        let _permit = self
            .semaphore
            .acquire()
            .await
            // SAFETY: this should not panic as we never close the semaphore
            .expect("Semaphore was closed unexpectedly");

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
                return Err(ErrorKind::NetworkRequest(e));
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

        if let (Some(remaining), Some(limit)) = (remaining, limit)
            && limit > 0
        {
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

        // Check for Retry-After header (in seconds)
        if let Some(retry_after_value) = headers.get("retry-after")
            && let Ok(retry_after_str) = retry_after_value.to_str()
            && let Ok(retry_seconds) = retry_after_str.parse::<u64>()
        {
            let mut backoff = self.backoff_duration.lock().unwrap();
            let retry_duration = Duration::from_secs(retry_seconds);
            // Cap retry-after to reasonable limits
            if retry_duration <= Duration::from_secs(3600) {
                *backoff = std::cmp::max(*backoff, retry_duration);
            }
        }
    }

    /// Helper method to parse numeric header values from common rate limit headers
    fn parse_header_value(headers: &http::HeaderMap, header_names: &[&str]) -> Option<usize> {
        for header_name in header_names {
            if let Some(value) = headers.get(*header_name)
                && let Ok(value_str) = value.to_str()
                && let Ok(number) = value_str.parse::<usize>()
            {
                return Some(number);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ratelimit::{HostConfig, RateLimitConfig};
    use reqwest::Client;

    #[tokio::test]
    async fn test_host_creation() {
        let key = HostKey::from("example.com");
        let host_config = HostConfig::default();
        let global_config = RateLimitConfig::default();

        let host = Host::new(key.clone(), &host_config, &global_config, Client::default()).unwrap();

        assert_eq!(host.key, key);
        assert_eq!(host.available_permits(), 10); // Default concurrency
        assert!((host.stats().success_rate() - 1.0).abs() < f64::EPSILON);
        assert_eq!(host.cache_size(), 0);
    }
}
