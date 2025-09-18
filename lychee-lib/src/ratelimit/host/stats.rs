use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::ratelimit::window::Window;

/// Statistics tracking for a host's request patterns
#[derive(Debug, Clone, Default)]
pub struct HostStats {
    /// Total number of requests made to this host
    pub total_requests: u64,
    /// Number of successful requests (2xx status)
    pub successful_requests: u64,
    /// Number of requests that received rate limit responses (429)
    pub rate_limited: u64,
    /// Number of server error responses (5xx)
    pub server_errors: u64,
    /// Number of client error responses (4xx, excluding 429)
    pub client_errors: u64,
    /// Timestamp of the last successful request
    pub last_success: Option<Instant>,
    /// Timestamp of the last rate limit response
    pub last_rate_limit: Option<Instant>,
    /// Request times for median calculation (kept in rolling window)
    pub request_times: Window<Duration>,
    /// Status code counts
    pub status_codes: HashMap<u16, u64>,
    /// Number of cache hits
    pub cache_hits: u64,
    /// Number of cache misses
    pub cache_misses: u64,
}

impl HostStats {
    /// Create new host statistics with custom window size for request times
    #[must_use]
    pub fn with_window_size(window_size: usize) -> Self {
        Self {
            request_times: Window::new(window_size),
            ..Default::default()
        }
    }

    /// Record a response with status code and request duration
    pub fn record_response(&mut self, status_code: u16, request_time: Duration) {
        self.total_requests += 1;

        // Track status code
        *self.status_codes.entry(status_code).or_insert(0) += 1;

        // Categorize response
        match status_code {
            200..=299 => {
                self.successful_requests += 1;
                self.last_success = Some(Instant::now());
            }
            429 => {
                self.rate_limited += 1;
                self.last_rate_limit = Some(Instant::now());
            }
            400..=499 => {
                self.client_errors += 1;
            }
            500..=599 => {
                self.server_errors += 1;
            }
            _ => {} // Other status codes
        }

        // Track request time in rolling window
        self.request_times.push(request_time);
    }

    /// Get median request time
    #[must_use]
    pub fn median_request_time(&self) -> Option<Duration> {
        if self.request_times.is_empty() {
            return None;
        }

        let mut times = self.request_times.to_vec();
        times.sort();
        let mid = times.len() / 2;

        if times.len() % 2 == 0 {
            // Average of two middle values
            Some((times[mid - 1] + times[mid]) / 2)
        } else {
            Some(times[mid])
        }
    }

    /// Get error rate (percentage)
    #[must_use]
    pub fn error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            return 0.0;
        }
        let errors = self.rate_limited + self.client_errors + self.server_errors;
        #[allow(clippy::cast_precision_loss)]
        let error_rate = errors as f64 / self.total_requests as f64;
        error_rate * 100.0
    }

    /// Get the current success rate (0.0 to 1.0)
    #[must_use]
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            1.0 // Assume success until proven otherwise
        } else {
            #[allow(clippy::cast_precision_loss)]
            let success_rate = self.successful_requests as f64 / self.total_requests as f64;
            success_rate
        }
    }

    /// Get average request time
    #[must_use]
    pub fn average_request_time(&self) -> Option<Duration> {
        if self.request_times.is_empty() {
            return None;
        }

        let total: Duration = self.request_times.iter().sum();
        #[allow(clippy::cast_possible_truncation)]
        Some(total / (self.request_times.len() as u32))
    }

    /// Get the most recent request time
    #[must_use]
    pub fn latest_request_time(&self) -> Option<Duration> {
        self.request_times.iter().last().copied()
    }

    /// Check if this host has been experiencing rate limiting recently
    #[must_use]
    pub fn is_currently_rate_limited(&self) -> bool {
        if let Some(last_rate_limit) = self.last_rate_limit {
            // Consider rate limited if we got a 429 in the last 60 seconds
            last_rate_limit.elapsed() < Duration::from_secs(60)
        } else {
            false
        }
    }

    /// Record a cache hit
    pub const fn record_cache_hit(&mut self) {
        self.cache_hits += 1;
        // Cache hits should also count as total requests from user perspective
        self.total_requests += 1;
        // Cache hits are typically for successful previous requests, so count as successful
        self.successful_requests += 1;
    }

    /// Record a cache miss
    pub const fn record_cache_miss(&mut self) {
        self.cache_misses += 1;
        // Cache misses will be followed by actual requests that increment total_requests
        // so we don't increment here to avoid double-counting
    }

    /// Get cache hit rate (0.0 to 1.0)
    #[must_use]
    pub fn cache_hit_rate(&self) -> f64 {
        let total_cache_requests = self.cache_hits + self.cache_misses;
        if total_cache_requests == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let hit_rate = self.cache_hits as f64 / total_cache_requests as f64;
            hit_rate
        }
    }

    /// Get human-readable summary of the stats
    #[must_use]
    pub fn summary(&self) -> String {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let success_pct = (self.success_rate() * 100.0) as u64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let error_pct = self.error_rate() as u64;

        let avg_time = self
            .average_request_time()
            .map_or_else(|| "N/A".to_string(), |d| format!("{:.0}ms", d.as_millis()));

        format!(
            "{} requests ({}% success, {}% errors), avg: {}",
            self.total_requests, success_pct, error_pct, avg_time
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_host_stats_success_rate() {
        let mut stats = HostStats::default();

        // No requests yet - should assume success
        assert!((stats.success_rate() - 1.0).abs() < f64::EPSILON);

        // Record some successful requests
        stats.record_response(200, Duration::from_millis(100));
        stats.record_response(200, Duration::from_millis(120));
        assert!((stats.success_rate() - 1.0).abs() < f64::EPSILON);

        // Record a rate limited request
        stats.record_response(429, Duration::from_millis(150));
        assert!((stats.success_rate() - (2.0 / 3.0)).abs() < 0.001);

        // Record a server error
        stats.record_response(500, Duration::from_millis(200));
        assert!((stats.success_rate() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_host_stats_tracking() {
        let mut stats = HostStats::default();

        // Initially empty
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.successful_requests, 0);
        assert!(stats.error_rate().abs() < f64::EPSILON);

        // Record a successful response
        stats.record_response(200, Duration::from_millis(100));
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.successful_requests, 1);
        assert!(stats.error_rate().abs() < f64::EPSILON);
        assert_eq!(stats.status_codes.get(&200), Some(&1));

        // Record rate limited response
        stats.record_response(429, Duration::from_millis(200));
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.rate_limited, 1);
        assert!((stats.error_rate() - 50.0).abs() < f64::EPSILON);

        // Record server error
        stats.record_response(500, Duration::from_millis(150));
        assert_eq!(stats.total_requests, 3);
        assert_eq!(stats.server_errors, 1);

        // Check median request time
        assert_eq!(
            stats.median_request_time(),
            Some(Duration::from_millis(150))
        );
    }

    #[test]
    fn test_window_integration() {
        let mut stats = HostStats::with_window_size(2);

        stats.record_response(200, Duration::from_millis(100));
        stats.record_response(200, Duration::from_millis(200));
        stats.record_response(200, Duration::from_millis(300));

        // Window should only keep last 2 times
        assert_eq!(stats.request_times.len(), 2);

        let times: Vec<_> = stats.request_times.iter().copied().collect();
        assert_eq!(
            times,
            vec![Duration::from_millis(200), Duration::from_millis(300)]
        );
    }

    #[test]
    fn test_summary_formatting() {
        let mut stats = HostStats::default();
        stats.record_response(200, Duration::from_millis(150));
        stats.record_response(500, Duration::from_millis(200));

        let summary = stats.summary();
        assert!(summary.contains("2 requests"));
        assert!(summary.contains("50% success"));
        assert!(summary.contains("50% errors"));
        assert!(summary.contains("175ms")); // average of 150 and 200
    }
}
