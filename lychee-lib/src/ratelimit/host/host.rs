use crate::{ratelimit::CacheableResponse, retry::RetryExt};
use dashmap::DashMap;
use governor::{
    Quota, RateLimiter,
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
};
use http::{Method, StatusCode};
use humantime_serde::re::humantime::format_duration;
use log::{debug, warn};
use reqwest::{Client as ReqwestClient, Request, Response as ReqwestResponse};
use std::{
    num::NonZeroU32,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;

use super::key::HostKey;
use super::stats::HostStats;
use crate::Uri;
use crate::types::Result;
use crate::{
    ErrorKind,
    ratelimit::{HostConfig, HttpClients, RateLimitConfig},
    retry::requires_http1_fallback,
};

/// Cap maximum backoff duration to reasonable limits
const MAXIMUM_BACKOFF: Duration = Duration::from_secs(60);

/// Identifies a request for caching and de-duplication purposes.
///
/// The [`Method`] is part of the key because the same URL can yield different
/// results per method (e.g. a server may reject `HEAD` but accept `GET`).
/// Keying by method ensures method fallback re-requests instead of reusing a
/// previous method's cached response.
type RequestKey = (Method, Uri);

/// Per-host cache for storing request results.
type HostCache = DashMap<RequestKey, CacheableResponse>;

/// Represents a single host with its own rate limiting, concurrency control,
/// HTTP client configuration, and request cache.
///
/// Each host maintains:
/// - A token bucket rate limiter using governor
/// - A semaphore for concurrency control
/// - Dedicated HTTP clients with host-specific headers, cookies, and HTTP/1.1 fallback
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

    /// HTTP clients configured for this specific host.
    clients: HttpClients,

    /// Whether HTTP/1.1 fallback has succeeded for this host.
    prefer_http1: AtomicBool,

    /// Request statistics and adaptive behavior tracking
    stats: Mutex<HostStats>,

    /// Current backoff duration for adaptive rate limiting
    backoff_duration: Mutex<Duration>,

    /// Per-host cache to prevent duplicate requests during a single link check invocation.
    /// Note that this cache has no direct relation to the inter-process persistable [`crate::CacheStatus`].
    cache: HostCache,

    /// Keep track of currently active requests, to prevent duplicate concurrent requests
    active_requests: DashMap<RequestKey, Arc<tokio::sync::Mutex<()>>>,
}

impl Host {
    /// Create a new Host instance for the given hostname.
    pub(crate) fn new(
        key: HostKey,
        host_config: &HostConfig,
        global_config: &RateLimitConfig,
        clients: HttpClients,
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
            clients,
            prefer_http1: AtomicBool::new(false),
            stats: Mutex::new(HostStats::default()),
            backoff_duration: Mutex::new(Duration::from_millis(0)),
            cache: DashMap::new(),
            active_requests: DashMap::new(),
        }
    }

    /// Check if a request is cached and returns the cached response if it is
    /// valid and satisfies the `needs_body` requirement.
    fn get_cached_status(&self, key: &RequestKey, needs_body: bool) -> Option<CacheableResponse> {
        let cached = self.cache.get(key)?.clone();
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

    /// Record a cache hit from the persistent disk cache.
    /// Cache misses are tracked internally, so we don't expose such a method.
    ///
    /// # Panics
    ///
    /// Panics if the internal stats mutex is poisoned.
    pub fn record_cache_hit(&self) {
        self.stats
            .lock()
            .expect("Stats mutex is poisoned")
            .record_cache_hit();
    }

    fn record_cache_miss(&self) {
        self.stats.lock().unwrap().record_cache_miss();
    }

    /// Cache a request result
    fn cache_result(&self, key: RequestKey, response: CacheableResponse) {
        // Do not cache responses that are potentially retried
        if !response.status.should_retry() {
            self.cache.insert(key, response);
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
        let method = request.method().clone();
        let mut url = request.url().clone();
        url.set_fragment(None);
        let key = (method, Uri::from(url));
        let _uri_guard = self.lock_uri_mutex(key.clone()).await;

        if let Some(cached) = self.get_cached_status(&key, needs_body) {
            self.record_cache_hit();
            return Ok(cached);
        }

        self.record_cache_miss();
        let _permit = self.acquire_semaphore().await;

        self.await_backoff().await;

        if let Some(rate_limiter) = &self.rate_limiter {
            rate_limiter.until_ready().await;
        }

        self.perform_request_with_fallback(request, key, needs_body)
            .await
    }

    pub(crate) const fn get_client(&self) -> &ReqwestClient {
        &self.clients.default_client
    }

    async fn perform_request_with_fallback(
        &self,
        request: Request,
        key: RequestKey,
        needs_body: bool,
    ) -> Result<CacheableResponse> {
        let Some(http1_fallback_client) = self.clients.http1_fallback_client.as_ref() else {
            return self
                .perform_request(&self.clients.default_client, request, key, needs_body)
                .await;
        };

        if self.prefer_http1.load(Ordering::Relaxed) {
            return self
                .perform_request(http1_fallback_client, request, key, needs_body)
                .await;
        }

        let Some(http1_request) = request.try_clone() else {
            return self
                .perform_request(&self.clients.default_client, request, key, needs_body)
                .await;
        };
        let response = self
            .perform_request(
                &self.clients.default_client,
                request,
                key.clone(),
                needs_body,
            )
            .await;

        if !response.as_ref().is_err_and(requires_http1_fallback) {
            return response;
        }

        debug!(
            "HTTP/2 failed for {}; retrying over HTTP/1.1",
            http1_request.url()
        );
        let response = self
            .perform_request(http1_fallback_client, http1_request, key, needs_body)
            .await;
        if response.is_ok() {
            self.prefer_http1.store(true, Ordering::Relaxed);
        }
        response
    }

    async fn perform_request(
        &self,
        client: &ReqwestClient,
        request: Request,
        key: RequestKey,
        needs_body: bool,
    ) -> Result<CacheableResponse> {
        let start_time = Instant::now();
        let response = match client.execute(request).await {
            Ok(response) => response,
            Err(e) => {
                // Record the network error in the per-host totals.
                self.stats
                    .lock()
                    .unwrap()
                    .record_network_error(start_time.elapsed());
                // Wrap network/HTTP errors to preserve the original error
                return Err(ErrorKind::NetworkRequest(e));
            }
        };

        self.update_stats(response.status(), start_time.elapsed());
        self.update_backoff(response.status());
        self.parse_rate_limit_headers(&response);

        let response = CacheableResponse::from_response(response, needs_body).await?;
        self.cache_result(key, response.clone());
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

    /// Get a [`tokio::sync::OwnedMutexGuard<()>`]
    /// to prevent concurrent identical requests (same method and [`Uri`]).
    async fn lock_uri_mutex(&self, key: RequestKey) -> tokio::sync::OwnedMutexGuard<()> {
        let uri_mutex = self
            .active_requests
            .entry(key)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();

        uri_mutex.lock_owned().await
    }

    /// Enforce the maximum concurrency of this host
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
            .expect("Stats mutex is poisoned")
            .record_response(status.as_u16(), request_time);
    }

    /// Parse rate limit headers from response and adjust behavior
    fn parse_rate_limit_headers(&self, response: &ReqwestResponse) {
        let headers = response.headers();

        if let Ok(rate_limit) = rate_limits::RateLimit::new(headers)
            && rate_limit.is_limited()
        {
            let duration = rate_limit.reset().duration();
            if !duration.is_zero() {
                self.increase_backoff(duration);
            }
        }
    }

    /// Increase backoff duration to the given duration, but cap it to a
    /// reasonable maximum to prevent excessively long backoffs.
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
        // Take the maximum of the current backoff and the new backoff to avoid
        // accidentally reducing the backoff duration
        *backoff = std::cmp::max(*backoff, increased_backoff);
    }

    /// Get host statistics
    ///
    /// # Panics
    ///
    /// Panics if the statistics mutex is poisoned
    pub fn stats(&self) -> HostStats {
        self.stats.lock().expect("Stats mutex is poisoned").clone()
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
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[tokio::test]
    async fn test_host_creation() {
        let key = HostKey::from("example.com");
        let host_config = HostConfig::default();
        let global_config = RateLimitConfig::default();

        let host = Host::new(
            key.clone(),
            &host_config,
            &global_config,
            HttpClients::new(Client::default()),
        );

        assert_eq!(host.key, key);
        assert_eq!(host.semaphore.available_permits(), 10); // Default concurrency
        assert!((host.stats().success_rate() - 1.0).abs() < f64::EPSILON);
        assert_eq!(host.cache_size(), 0);
    }

    #[tokio::test]
    async fn retries_http2_protocol_errors_over_http1_and_remembers_preference() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut http2, _) = listener.accept().await.unwrap();
            let mut preface = [0; 24];
            http2.read_exact(&mut preface).await.unwrap();
            assert_eq!(&preface, b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
            http2
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
            drop(http2);

            let (mut http1, _) = listener.accept().await.unwrap();
            let mut request = [0; 1024];
            let length = http1.read(&mut request).await.unwrap();
            assert!(request[..length].starts_with(b"GET / HTTP/1.1\r\n"));
            http1
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();

            let (mut preferred, _) = listener.accept().await.unwrap();
            let mut request = [0; 1024];
            let length = preferred.read(&mut request).await.unwrap();
            assert!(request[..length].starts_with(b"GET /next HTTP/1.1\r\n"));
            preferred
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
        });

        let default_client = Client::builder().http2_prior_knowledge().build().unwrap();
        let clients = HttpClients::with_http1_fallback(default_client, Client::new());
        let host = Host::new(
            HostKey::from("127.0.0.1"),
            &HostConfig::default(),
            &RateLimitConfig::default(),
            clients,
        );

        let url = format!("http://{address}/").parse().unwrap();
        let response = host
            .execute_request(Request::new(Method::GET, url), false)
            .await
            .unwrap();
        assert!(response.status.is_success());

        let next_url = format!("http://{address}/next").parse().unwrap();
        let next_response = host
            .execute_request(Request::new(Method::GET, next_url), false)
            .await
            .unwrap();
        assert!(next_response.status.is_success());

        server.await.unwrap();
    }
}
