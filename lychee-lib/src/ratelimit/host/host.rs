use crate::{
    ratelimit::{CacheableResponse, headers},
    retry::RetryExt,
};
use dashmap::DashMap;
use governor::{
    Quota, RateLimiter,
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
};
use http::StatusCode;
use humantime_serde::re::humantime::format_duration;
use log::warn;
use reqwest::{Client as ReqwestClient, Request, Response as ReqwestResponse};
use std::time::{Duration, Instant};
use std::{num::NonZeroU32, sync::Mutex};
use tokio::sync::Semaphore;

use super::key::HostKey;
use super::stats::HostStats;
use crate::Uri;
use crate::types::Result;
use crate::{
    ErrorKind,
    ratelimit::{HostConfig, RateLimitConfig},
};

/// Cap maximum backoff duration to reasonable limits
const MAXIMUM_BACKOFF: Duration = Duration::from_secs(60);

/// Per-host cache for storing request results
type HostCache = DashMap<Uri, CacheableResponse>;

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
    rate_limiter: Option<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,

    /// Controls maximum concurrent requests to this host
    semaphore: Semaphore,

    /// HTTP client configured for this specific host
    client: ReqwestClient,

    /// Request statistics and adaptive behavior tracking
    stats: Mutex<HostStats>,

    /// Current backoff duration for adaptive rate limiting
    backoff_duration: Mutex<Duration>,

    /// Per-host cache to prevent duplicate requests during a single link check invocation.
    /// Note that this cache has no direct relation to the inter-process persistable [`crate::CacheStatus`].
    cache: HostCache,
}

impl Host {
    /// Create a new Host instance for the given hostname
    #[must_use]
    pub fn new(
        key: HostKey,
        host_config: &HostConfig,
        global_config: &RateLimitConfig,
        client: ReqwestClient,
    ) -> Self {
        const MAX_BURST: NonZeroU32 = NonZeroU32::new(1).unwrap();
        let interval = host_config.effective_request_interval(global_config);
        let rate_limiter =
            Quota::with_period(interval).map(|q| RateLimiter::direct(q.allow_burst(MAX_BURST)));

        // Create semaphore for concurrency control
        let max_concurrent = host_config.effective_concurrency(global_config);
        let semaphore = Semaphore::new(max_concurrent);

        Host {
            key,
            rate_limiter,
            semaphore,
            client,
            stats: Mutex::new(HostStats::default()),
            backoff_duration: Mutex::new(Duration::from_millis(0)),
            cache: DashMap::new(),
        }
    }

    /// Check if a URI is cached and returns the cached response if it is valid
    /// and satisfies the `needs_body` requirement.
    fn get_cached_status(&self, uri: &Uri, needs_body: bool) -> Option<CacheableResponse> {
        let cached = self.cache.get(uri)?.clone();
        if needs_body {
            if cached.text.is_some() {
                Some(cached)
            } else {
                None
            }
        } else {
            Some(cached)
        }
    }

    fn record_cache_hit(&self) {
        self.stats.lock().unwrap().record_cache_hit();
    }

    fn record_cache_miss(&self) {
        self.stats.lock().unwrap().record_cache_miss();
    }

    /// Cache a request result
    fn cache_result(&self, uri: &Uri, response: CacheableResponse) {
        // Do not cache responses that are potentially retried
        if !response.status.should_retry() {
            self.cache.insert(uri.clone(), response);
        }
    }

    /// Execute a request with rate limiting, concurrency control, and caching
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or rate limiting is exceeded
    ///
    /// # Panics
    ///
    /// Panics if the statistics mutex is poisoned
    pub(crate) async fn execute_request(
        &self,
        request: Request,
        needs_body: bool,
    ) -> Result<CacheableResponse> {
        let mut url = request.url().clone();
        url.set_fragment(None);
        let uri = Uri::from(url);

        let _permit = self.acquire_semaphore().await;

        if let Some(cached) = self.get_cached_status(&uri, needs_body) {
            self.record_cache_hit();
            return Ok(cached);
        }

        self.await_backoff().await;

        if let Some(rate_limiter) = &self.rate_limiter {
            rate_limiter.until_ready().await;
        }

        if let Some(cached) = self.get_cached_status(&uri, needs_body) {
            self.record_cache_hit();
            return Ok(cached);
        }

        self.record_cache_miss();
        self.perform_request(request, uri, needs_body).await
    }

    pub(crate) const fn get_client(&self) -> &ReqwestClient {
        &self.client
    }

    async fn perform_request(
        &self,
        request: Request,
        uri: Uri,
        needs_body: bool,
    ) -> Result<CacheableResponse> {
        let start_time = Instant::now();
        let response = match self.client.execute(request).await {
            Ok(response) => response,
            Err(e) => {
                // Wrap network/HTTP errors to preserve the original error
                return Err(ErrorKind::NetworkRequest(e));
            }
        };

        self.update_stats(response.status(), start_time.elapsed());
        self.update_backoff(response.status());
        self.handle_rate_limit_headers(&response);

        let response = CacheableResponse::from_response(response, needs_body).await?;
        self.cache_result(&uri, response.clone());
        Ok(response)
    }

    /// Await adaptive backoff if needed
    async fn await_backoff(&self) {
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
    }

    async fn acquire_semaphore(&self) -> tokio::sync::SemaphorePermit<'_> {
        self.semaphore
            .acquire()
            .await
            // SAFETY: this should not panic as we never close the semaphore
            .expect("Semaphore was closed unexpectedly")
    }

    fn update_backoff(&self, status: StatusCode) {
        let mut backoff = self.backoff_duration.lock().unwrap();
        match status.as_u16() {
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

    fn update_stats(&self, status: StatusCode, request_time: Duration) {
        self.stats
            .lock()
            .unwrap()
            .record_response(status.as_u16(), request_time);
    }

    /// Parse rate limit headers from response and adjust behavior
    fn handle_rate_limit_headers(&self, response: &ReqwestResponse) {
        // Implement basic parsing here rather than using the rate-limits crate to keep dependencies minimal
        let headers = response.headers();
        self.handle_retry_after_header(headers);
        self.handle_common_rate_limit_header_fields(headers);
    }

    /// Handle the common "X-RateLimit" header fields.
    fn handle_common_rate_limit_header_fields(&self, headers: &http::HeaderMap) {
        if let (Some(remaining), Some(limit)) =
            headers::parse_common_rate_limit_header_fields(headers)
            && limit > 0
        {
            #[allow(clippy::cast_precision_loss)]
            let usage_ratio = limit.saturating_sub(remaining) as f64 / limit as f64;

            // If we've used more than 80% of our quota, apply preventive backoff
            if usage_ratio > 0.8 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let duration = Duration::from_millis((200.0 * (usage_ratio - 0.8) / 0.2) as u64);
                self.increase_backoff(duration);
            }
        }
    }

    /// Handle the "Retry-After" header
    fn handle_retry_after_header(&self, headers: &http::HeaderMap) {
        if let Some(retry_after_value) = headers.get("retry-after") {
            let duration = match headers::parse_retry_after(retry_after_value) {
                Ok(e) => e,
                Err(e) => {
                    warn!("Unable to parse Retry-After header as per RFC 7231: {e}");
                    return;
                }
            };

            self.increase_backoff(duration);
        }
    }

    fn increase_backoff(&self, mut increased_backoff: Duration) {
        if increased_backoff > MAXIMUM_BACKOFF {
            warn!(
                "Host {} sent an unexpectedly big rate limit backoff duration of {}. Capping the duration to {} instead.",
                self.key,
                format_duration(increased_backoff),
                format_duration(MAXIMUM_BACKOFF)
            );
            increased_backoff = MAXIMUM_BACKOFF;
        }

        let mut backoff = self.backoff_duration.lock().unwrap();
        *backoff = std::cmp::max(*backoff, increased_backoff);
    }

    /// Get host statistics
    ///
    /// # Panics
    ///
    /// Panics if the statistics mutex is poisoned
    pub fn stats(&self) -> HostStats {
        self.stats.lock().unwrap().clone()
    }

    /// Record a cache hit from the persistent disk cache.
    /// Cache misses are tracked internally, so we don't expose such a method.
    pub(crate) fn record_persistent_cache_hit(&self) {
        self.record_cache_hit();
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

        let host = Host::new(key.clone(), &host_config, &global_config, Client::default());

        assert_eq!(host.key, key);
        assert_eq!(host.semaphore.available_permits(), 10); // Default concurrency
        assert!((host.stats().success_rate() - 1.0).abs() < f64::EPSILON);
        assert_eq!(host.cache_size(), 0);
    }
}
