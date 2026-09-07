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

pub use config::{HostConfig, HostConfigs, RateLimitConfig};
pub use host::{Host, HostKey, HostStats, HostStatsMap};
use http::HeaderMap;
pub use pool::HostPool;
use reqwest::{Client, Response};
use std::collections::HashMap;
use url::Url;

use crate::{ErrorKind, Result};

#[derive(Debug, Clone)]
pub(crate) struct HttpClients {
    pub(crate) default_client: Client,
    pub(crate) http1_fallback_client: Option<Client>,
}

impl HttpClients {
    pub(crate) const fn new(default_client: Client) -> Self {
        Self {
            default_client,
            http1_fallback_client: None,
        }
    }

    pub(crate) const fn with_http1_fallback(
        default_client: Client,
        http1_fallback_client: Client,
    ) -> Self {
        Self {
            default_client,
            http1_fallback_client: Some(http1_fallback_client),
        }
    }
}

pub(crate) type HostClientMap = HashMap<HostKey, HttpClients>;

/// The result of a HTTP request, used for internal per-host caching.
/// This abstraction exists, because [`Response`] cannot easily be cached
/// since it does not implement [`Clone`].
#[derive(Debug, Clone)]
pub(crate) struct CacheableResponse {
    /// HTTP status code of the response.
    pub(crate) status: reqwest::StatusCode,
    /// Response body text. Only populated when `needs_body` was `true` in
    /// [`HostPool::execute_request`].
    pub(crate) text: Option<String>,
    /// Response headers.
    pub(crate) headers: HeaderMap,
    /// Final URL after any redirects.
    pub(crate) url: Url,
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
