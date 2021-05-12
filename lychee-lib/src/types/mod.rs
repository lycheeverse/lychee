#![allow(unreachable_pub)]

mod error;
mod request;
mod response;
mod status;

pub use error::ErrorKind;
pub use request::Request;
pub use response::{Response, ResponseBody};
pub use status::Status;

/// The lychee `Result` type
pub type Result<T> = std::result::Result<T, crate::ErrorKind>;
