//! Per-host rate limiting and concurrency control.
//!
//! This module provides adaptive rate limiting for HTTP requests on a per-host basis.
//! It prevents overwhelming servers with too many concurrent requests and respects
//! server-provided rate limit headers.
//!
//! # Architecture
//!
//! - [`HostKey`]: Represents a hostname/domain for rate limiting
//! - [`Host`]: Manages rate limiting, concurrency, and caching for a specific host
//! - [`HostPool`]: Coordinates multiple hosts and routes requests appropriately
//! - [`HostConfig`]: Configuration for per-host behavior
//! - [`HostStats`]: Statistics tracking for each host

mod config;
mod headers;
mod host;
mod pool;

pub use config::{HostConfig, HostConfigs, RateLimitConfig};
pub use host::{Host, HostKey, HostStats, HostStatsMap};
use http::HeaderMap;
pub use pool::{ClientMap, HostPool};
use reqwest::Response;
use url::Url;

use crate::{ErrorKind, Result};

/// The result of a HTTP request, used for internal per-host caching.
/// This abstraction exists, because [`Response`] cannot easily be cached
/// since it does not implement [`Clone`].
#[derive(Debug, Clone)]
pub(crate) struct CacheableResponse {
    pub(crate) status: reqwest::StatusCode,
    pub(crate) text: Option<String>,
    pub(crate) headers: HeaderMap,
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
