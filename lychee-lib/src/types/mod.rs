#![allow(unreachable_pub)]

mod base;
mod cache;
mod error;
mod file;
mod input;
pub(crate) mod raw_uri;
mod request;
mod response;
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

/// lychee supports recursion up to an arbitrary depth.
/// In order to keep track of the current level of recursion,
/// it gets stored in the input and response objects
/// 
/// Setting the level to `-1` means infinite recursion
type RecursionLevel = isize;
