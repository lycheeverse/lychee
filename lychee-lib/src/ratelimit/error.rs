use thiserror::Error;
use url::Url;

use crate::ratelimit::HostKey;

/// Errors that can occur during rate limiting operations
#[derive(Error, Debug)]
pub enum RateLimitError {
    /// Host exceeded its rate limit
    #[error("Host {host} exceeded rate limit: {message}")]
    RateLimitExceeded {
        /// The host that exceeded the limit
        host: HostKey,
        /// Additional context message
        message: String,
    },

    /// User specified an invalid rate limit interval
    #[error("Invalid rate limit interval for host {host}")]
    InvalidRateLimitInterval {
        /// The host with invalid configuration
        host: HostKey,
    },

    /// Failed to parse rate limit headers from server response
    #[error("Failed to parse URL {url}: {reason}")]
    UrlParseError {
        /// The host that sent invalid headers
        url: Url,
        /// Reason for parse failure
        reason: String,
    },

    /// Error creating or configuring HTTP client for host
    #[error("Failed to configure client for host {host}: {source}")]
    ClientConfigError {
        /// The host that failed configuration
        host: HostKey,
        /// Underlying error
        source: reqwest::Error,
    },

    /// Cookie store operation failed
    #[error("Cookie operation failed for host {host}: {reason}")]
    CookieError {
        /// The host with cookie issues
        host: HostKey,
        /// Description of cookie error
        reason: String,
    },

    /// Network error occurred during request execution
    #[error("Network error for host {host}: {source}")]
    NetworkError {
        /// The host that had the network error
        host: HostKey,
        /// The underlying network error
        #[source]
        source: reqwest::Error,
    },
}
