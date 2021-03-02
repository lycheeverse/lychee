//!
//! `lychee` is a library for checking links.  
//! It is asynchronous and supports multiple input formats like Markdown and HTML.
//! Here is a basic usage example:
//!
//! ```
//! use std::error::Error;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn Error>> {
//!   let response = lychee::check("https://github.com/lycheeverse/lychee").await?;
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
//! use lychee::{ClientBuilder, Status};
//! use std::error::Error;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn Error>> {
//!   let client = ClientBuilder::default().build()?;
//!   let response = client.check("https://github.com/lycheeverse/lychee").await?;
//!   assert!(matches!(response.status, Status::Ok(_)));
//!   Ok(())
//! }
//! ```

#[deny(missing_docs)]
#[cfg(doctest)]
#[macro_use]
extern crate doc_comment;

#[cfg(doctest)]
doctest!("../README.md");

mod client;
mod client_pool;
mod filter;
mod types;
mod uri;

pub mod collector;
pub mod extract;
pub mod test_utils;

pub use client::check;
pub use client::ClientBuilder;
pub use client_pool::ClientPool;
pub use collector::Input;
pub use types::*;
pub use uri::Uri;
