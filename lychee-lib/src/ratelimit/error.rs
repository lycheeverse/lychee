use thiserror::Error;

/// Errors that can occur during rate limiting operations
#[derive(Error, Debug)]
pub enum RateLimitError {
    /// Host exceeded its rate limit
    #[error("Host {host} exceeded rate limit: {message}")]
    RateLimitExceeded {
        /// The host that exceeded the limit
        host: String,
        /// Additional context message
        message: String,
    },

    /// Failed to parse rate limit headers from server response
    #[error("Failed to parse rate limit headers from {host}: {reason}")]
    HeaderParseError {
        /// The host that sent invalid headers
        host: String,
        /// Reason for parse failure
        reason: String,
    },

    /// Error creating or configuring HTTP client for host
    #[error("Failed to configure client for host {host}: {source}")]
    ClientConfigError {
        /// The host that failed configuration
        host: String,
        /// Underlying error
        source: reqwest::Error,
    },

    /// Cookie store operation failed
    #[error("Cookie operation failed for host {host}: {reason}")]
    CookieError {
        /// The host with cookie issues
        host: String,
        /// Description of cookie error
        reason: String,
    },

    /// Network error occurred during request execution
    #[error("Network error for host {host}: {source}")]
    NetworkError {
        /// The host that had the network error
        host: String,
        /// The underlying network error
        #[source]
        source: reqwest::Error,
    },
}
