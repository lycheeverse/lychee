//! `lychee` is a library for checking links.
//! "Hello world" example:
//! ```
//! use lychee_lib::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!   let response = lychee_lib::check("https://github.com/lycheeverse/lychee").await?;
//!   println!("{}", response);
//!   Ok(())
//! }
//! ```
//!
//! For more specific use-cases you can build a lychee client yourself,
//! using the `ClientBuilder` which can be used to
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
#![warn(clippy::all, clippy::pedantic)]
#![warn(
    absolute_paths_not_starting_with_crate,
    invalid_html_tags,
    missing_copy_implementations,
    missing_debug_implementations,
    semicolon_in_expressions_from_macros,
    unreachable_pub,
    unused_crate_dependencies,
    unused_extern_crates,
    variant_size_differences,
    clippy::missing_const_for_fn
)]
#![deny(anonymous_parameters, macro_use_extern_crate, pointer_structural_match)]
#![deny(missing_docs)]
#![allow(clippy::module_name_repetitions)]

#[cfg(doctest)]
doc_comment::doctest!("../../README.md");

mod client;
mod client_pool;
/// A pool of clients, to handle concurrent checks
pub mod collector;
mod fs_tree;
mod quirks;
mod types;
mod uri;

/// Functionality to extract URIs from inputs
pub mod extract;

/// Filters are a way to define behavior when encountering
/// URIs that need to be treated differently, such as
/// local IPs or e-mail addresses
pub mod filter;

#[cfg(test)]
#[macro_use]
pub mod test_utils;

#[cfg(test)]
use doc_comment as _; // required for doctest
use openssl_sys as _; // required for vendored-openssl feature
use ring as _; // required for apple silicon

#[doc(inline)]
pub use crate::{
    client::{check, ClientBuilder},
    client_pool::ClientPool,
    collector::Collector,
    filter::{Excludes, Filter, Includes},
    types::{ErrorKind, Input, Request, Response, ResponseBody, Result, Status},
    uri::Uri,
};
