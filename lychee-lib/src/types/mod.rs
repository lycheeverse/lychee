#![allow(unreachable_pub)]

mod error;
mod file;
mod input;
mod request;
mod response;
mod status;

pub use error::ErrorKind;
pub use file::FileType;
pub use input::{Input, InputContent};
pub use request::Request;
pub use response::{Response, ResponseBody};
pub use status::Status;

/// The lychee `Result` type
pub type Result<T> = std::result::Result<T, crate::ErrorKind>;
