#![allow(unreachable_pub)]

mod base;
mod cache;
mod cookies;
mod error;
mod file;
mod input;
pub(crate) mod mail;
mod request;
mod response;
mod status;
pub(crate) mod uri;

pub use base::Base;
pub use cache::CacheStatus;
pub use cookies::CookieJar;
pub use error::ErrorKind;
pub use file::FileType;
pub use input::{Input, InputContent, InputSource};
pub use request::Request;
pub use response::{Response, ResponseBody};
pub use status::Status;

/// The lychee `Result` type
pub type Result<T> = std::result::Result<T, crate::ErrorKind>;
