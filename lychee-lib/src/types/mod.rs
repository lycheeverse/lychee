mod error;
mod request;
mod response;
mod status;

pub use error::ErrorKind;
pub use request::Request;
pub use response::{Response, ResponseBody};
pub use status::Status;

pub type Result<T> = std::result::Result<T, crate::ErrorKind>;
