#![allow(unreachable_pub)]

mod accept;
mod base_info;
mod basic_auth;
mod cache;
mod cookies;
mod error;
mod file;
pub(crate) mod hints;
mod input;
mod methods;
mod preprocessor;
pub(crate) mod redirect_history;
mod request;
mod request_error;
pub(crate) mod resolver;
mod response;
mod status;
mod status_code_selector;
pub(crate) mod uri;

pub use accept::*;
pub use base_info::BaseInfo;
pub use basic_auth::{BasicAuthCredentials, BasicAuthSelector};
pub use cache::CacheStatus;
pub use cookies::{CookieError, CookieJar};
pub use error::{ErrorKind, InternalError};
pub use file::{FileExtensions, FileType};
pub use input::{Input, InputContent, InputResolver, InputSource, ResolvedInputSource};
pub use methods::{Methods, MethodsError};
pub use preprocessor::{Preprocessor, PreprocessorError};
pub use redirect_history::{Redirect, Redirects};
pub use request::Request;
pub use request_error::RequestError;
pub use response::{Response, ResponseBody};
pub use status::Status;
pub use status_code_selector::*;
pub use uri::error::UriError;
pub use uri::github::GithubError;

/// The lychee `Result` type
pub type Result<T> = std::result::Result<T, crate::ErrorKind>;

/// The lychee `Result` type, aliased to avoid conflicting with [`std::result::Result`].
pub type LycheeResult<T> = Result<T>;
