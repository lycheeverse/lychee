//! Per-host rate limiting and concurrency control.
//!
//! This module provides adaptive rate limiting for HTTP requests on a per-host basis.
//! It prevents overwhelming servers with too many concurrent requests and respects
//! server-provided rate limit headers.
//!
//! # Architecture
//!
//! - [`HostKey`]: Represents a hostname/domain for rate limiting
//! - [`Host`]: Manages rate limiting, concurrency, caching, and cookies for a specific host
//! - [`HostPool`]: Coordinates multiple hosts and routes requests appropriately
//! - [`HostConfig`]: Configuration for per-host behavior
//! - [`HostStats`]: Statistics tracking for each host
//! - [`Window`]: Rolling window data structure for request timing

mod config;
mod error;
mod host;
mod pool;
mod window;

pub use config::{HostConfig, RateLimitConfig};
pub use error::RateLimitError;
pub use host::{Host, HostKey, HostStats};
pub use pool::HostPool;
pub use window::Window;
