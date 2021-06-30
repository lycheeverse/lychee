#![allow(unreachable_pub)]

mod base;
mod error;
mod file;
mod input;
mod request;
mod response;
mod status;
mod uri;

pub use base::Base;
pub use error::ErrorKind;
pub use file::FileType;
pub use input::{Input, InputContent};
pub use request::Request;
pub use response::{Response, ResponseBody};
pub use status::Status;
pub use uri::Uri;

/// The lychee `Result` type
pub type Result<T> = std::result::Result<T, crate::ErrorKind>;
