//! Per-host rate limiting and concurrency control.
//!
//! This module provides adaptive rate limiting for HTTP requests on a per-host basis.
//! It prevents overwhelming servers with too many concurrent requests and respects
//! server-provided rate limit headers.
//!
//! # Architecture
//!
//! - [`crate::ratelimit::HostKey`]: Represents a hostname/domain for rate limiting
//! - [`crate::ratelimit::Host`]: Manages rate limiting, concurrency, and caching for a specific host
//! - [`crate::ratelimit::HostPool`]: Coordinates multiple hosts and routes requests appropriately
//! - [`crate::ratelimit::HostConfig`]: Configuration for per-host behavior
//! - [`crate::ratelimit::HostStats`]: Statistics tracking for each host

mod config;
mod host;
mod pool;

use http::HeaderMap;
use reqwest::Response;
use url::Url;

use crate::{ErrorKind, Result};

pub use config::{HostConfig, HostConfigs, RateLimitConfig};
pub use host::{Host, HostKey, HostStats, HostStatsMap};
pub use pool::{ClientMap, HostPool};

/// The result of a HTTP request, used for internal per-host caching.
/// This abstraction exists, because [`Response`] cannot easily be cached
/// since it does not implement [`Clone`].
#[derive(Debug, Clone)]
pub(crate) struct CacheableResponse {
    /// Final URL after any redirects.
    pub(crate) url: Url,

    /// Response body text. Only populated when `needs_body` was `true` in
    /// [`HostPool::execute_request`].
    pub(crate) text: Option<String>,

    /// HTTP status code of the response.
    pub(crate) status: reqwest::StatusCode,

    /// Response headers.
    pub(crate) headers: HeaderMap,
}

impl CacheableResponse {
    async fn from_response(response: Response, needs_body: bool) -> Result<Self> {
        let status = response.status();
        let headers = response.headers().clone();
        let url = response.url().clone();
        let text = if needs_body {
            Some(response.text().await.map_err(ErrorKind::ReadResponseBody)?)
        } else {
            None
        };

        Ok(Self {
            status,
            text,
            headers,
            url,
        })
    }
}
