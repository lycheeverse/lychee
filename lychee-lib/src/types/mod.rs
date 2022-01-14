#![allow(unreachable_pub)]

mod base;
mod cache;
mod error;
mod file;
mod input;
pub(crate) mod mail;
pub(crate) mod raw_uri;
mod response;
mod request;
mod status;
mod uri;

pub use base::Base;
pub use cache::CacheStatus;
pub use error::ErrorKind;
pub use file::FileType;
pub use input::{Input, InputContent, InputSource};
pub use request::Request;
pub use response::{Response, ResponseBody};
pub use status::Status;
pub use uri::{GithubUri, Uri};

/// The lychee `Result` type
pub type Result<T> = std::result::Result<T, crate::ErrorKind>;
