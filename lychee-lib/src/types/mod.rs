#![allow(unreachable_pub)]

mod accept;
mod base;
mod basic_auth;
mod cache;
mod cookies;
mod error;
mod file;
mod input;
pub(crate) mod mail;
mod request;
pub(crate) mod resolver;
mod response;
mod status;
mod status_code;
pub(crate) mod uri;

pub use accept::*;
pub use base::Base;
pub use basic_auth::{BasicAuthCredentials, BasicAuthSelector};
pub use cache::CacheStatus;
pub use cookies::CookieJar;
pub use error::ErrorKind;
pub use file::{FileExtensions, FileType};
pub use input::{Input, InputContent, InputResolver, InputSource, ResolvedInputSource};
pub use request::Request;
pub use response::{Response, ResponseBody};
pub use status::Status;
pub use status_code::*;

/// The lychee `Result` type
pub type Result<T> = std::result::Result<T, crate::ErrorKind>;
