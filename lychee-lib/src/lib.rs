//! `lychee-lib` is the library component of [`lychee`], and is used for checking links.
//!
//! "Hello world" example:
//!
//! ```
//! use lychee_lib::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!   let response = lychee_lib::check("https://github.com/lycheeverse/lychee").await?;
//!   println!("{response}");
//!   Ok(())
//! }
//! ```
//!
//! For more specific use-cases you can build a lychee client yourself,
//! using the [`ClientBuilder`] which can be used to
//! configure and run your own link checker and grants full flexibility:
//!
//! ```
//! use lychee_lib::{ClientBuilder, Result, Status};
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!   let client = ClientBuilder::default().client()?;
//!   let response = client.check("https://github.com/lycheeverse/lychee").await?;
//!   assert!(response.status().is_success());
//!   Ok(())
//! }
//! ```
//!
//! [`lychee`]: https://github.com/lycheeverse/lychee
#![warn(clippy::all, clippy::pedantic)]
#![warn(
    absolute_paths_not_starting_with_crate,
    rustdoc::invalid_html_tags,
    missing_copy_implementations,
    missing_debug_implementations,
    semicolon_in_expressions_from_macros,
    unreachable_pub,
    unused_crate_dependencies,
    unused_extern_crates,
    variant_size_differences,
    clippy::missing_const_for_fn
)]
#![deny(anonymous_parameters, macro_use_extern_crate)]
#![deny(missing_docs)]
#![allow(clippy::module_name_repetitions)]

#[cfg(doctest)]
doc_comment::doctest!("../../README.md");

/// Check online archives to try and restore broken links
pub mod archive;
mod basic_auth;
pub mod chain;
mod checker;
mod client;
/// A pool of clients, to handle concurrent checks
pub mod collector;
mod quirks;
mod retry;
mod types;
mod utils;

/// Functionality to extract URIs from inputs
pub mod extract;

pub mod remap;

/// Filters are a way to define behavior when encountering
/// URIs that need to be treated differently, such as
/// local IPs or e-mail addresses
pub mod filter;

#[cfg(test)]
use doc_comment as _; // required for doctest
use ring as _; // required for apple silicon

#[cfg(feature = "native-tls")]
use openssl_sys as _; // required for vendored-openssl feature

#[doc(inline)]
pub use crate::{
    basic_auth::BasicAuthExtractor,
    // Expose the `Handler` trait to allow defining external handlers (plugins)
    chain::{ChainResult, Handler},
    // Constants get exposed so that the CLI can use the same defaults as the library
    client::{
        Client, ClientBuilder, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_RETRIES,
        DEFAULT_RETRY_WAIT_TIME_SECS, DEFAULT_TIMEOUT_SECS, DEFAULT_USER_AGENT, check,
    },
    collector::Collector,
    filter::{Excludes, Filter, Includes},
    types::{
        AcceptRange, AcceptRangeError, Base, BasicAuthCredentials, BasicAuthSelector, CacheStatus,
        CookieJar, ErrorKind, FileExtensions, FileType, Input, InputContent, InputResolver,
        InputSource, LycheeResult, Preprocessor, Redirects, Request, RequestError,
        ResolvedInputSource, Response, ResponseBody, Result, Status, StatusCodeExcluder,
        StatusCodeSelector, uri::raw::RawUri, uri::valid::Uri,
    },
};
