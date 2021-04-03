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
//!   let client = ClientBuilder::default().build()?;
//!   let response = client.check("https://github.com/lycheeverse/lychee").await?;
//!   assert!(response.status().is_success());
//!   Ok(())
//! }
//! ```
// #![deny(missing_docs)]

#[cfg(doctest)]
doc_comment::doctest!("../../README.md");

mod client;
mod client_pool;
mod filter;
mod quirks;
mod types;
mod uri;

pub mod collector;
pub mod extract;
#[cfg(test)]
#[macro_use]
pub mod test_utils;

pub use client::check;
pub use client::ClientBuilder;
pub use client_pool::ClientPool;
pub use collector::Input;
pub use types::*;
pub use uri::Uri;
