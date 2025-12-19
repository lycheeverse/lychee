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
pub use host::{Host, HostKey, HostStats};
pub use pool::{ClientMap, HostPool};
