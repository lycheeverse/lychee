use http::StatusCode;
use serde::{Serialize, Serializer};
use std::error::Error;
use std::hash::Hash;
use std::{convert::Infallible, path::PathBuf};
use thiserror::Error;
use tokio::task::JoinError;

use super::InputContent;
use crate::checker::mail::MailError;
use crate::checker::wikilink::WikilinkError;
use crate::remap::RemapError;
use crate::types::StatusCodeSelectorError;
use crate::types::cookies::CookieError;
use crate::types::preprocessor::PreprocessorError;
use crate::types::uri::error::UriError;
use crate::types::uri::github::GithubError;
use crate::{Uri, basic_auth::BasicAuthExtractorError, utils};

/// Internal, low-level errors that originate deep in the stack and are not
/// meaningful link-level diagnostics on their own (encoding, async runtime, and
/// internal channels). They are surfaced for completeness, but a user can rarely
/// act on them directly.
///
// TODO(#2097): Once `Status`/`Outcome` are split, these should be modelled as
// fatal application errors rather than per-link check failures.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum InternalError {
    /// A byte sequence could not be interpreted as valid UTF-8.
    #[error(
        "Encountered invalid UTF-8 sequence, while trying to interpret bytes UTF-8 string: {0}"
    )]
    Utf8(#[source] std::str::Utf8Error),

    /// A future failed to execute to completion on the Tokio runtime.
    #[error("Task failed to execute to completion: {0}")]
    RuntimeJoin(#[source] JoinError),

    /// Sending or receiving a message over an internal channel failed.
    #[error("Internal communication error, cannot send/receive message over channel: {0}")]
    Channel(#[source] tokio::sync::mpsc::error::SendError<InputContent>),
}

impl PartialEq for InternalError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Utf8(a), Self::Utf8(b)) => a == b,
            (Self::RuntimeJoin(a), Self::RuntimeJoin(b)) => a.to_string() == b.to_string(),
            (Self::Channel(_), Self::Channel(_)) => true,
            _ => false,
        }
    }
}

impl Eq for InternalError {}

impl Hash for InternalError {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Self::Utf8(e) => e.to_string().hash(state),
            Self::RuntimeJoin(e) => e.to_string().hash(state),
            Self::Channel(e) => e.to_string().hash(state),
        }
    }
}

/// Kinds of status errors
/// Note: The error messages can change over time, so don't match on the output
///
// TODO(#2097): This enum still owns request/response and status-code variants
// (`NetworkRequest`, `ReadResponseBody`, `BuildRequestClient`, `InvalidStatusCode`,
// `RejectedStatusCode`) plus a few config-parse leaves (`InvalidHeader`, `Regex`,
// `InsecureURL`). These are intentionally left here until the `Status`/`Outcome`
// split in https://github.com/lycheeverse/lychee/issues/2097, which restructures
// how request outcomes and status-code interpretation are modelled.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Network error while handling request.
    /// This does not include erroneous status codes, `RejectedStatusCode` will be used in that case.
    #[error("Network error: {analysis} ({error})", analysis=utils::reqwest::analyze_error_chain(.0), error=.0)]
    NetworkRequest(#[source] reqwest::Error),
    /// Cannot read the body of the received response
    #[error("Failed to read response body: {0}")]
    ReadResponseBody(#[source] reqwest::Error),
    /// The network client required for making requests cannot be created
    #[error("Failed to create HTTP request client: {0}")]
    BuildRequestClient(#[source] reqwest::Error),

    /// Error while checking a GitHub URL through the GitHub API
    #[error(transparent)]
    Github(#[from] GithubError),

    /// Error while converting a file to an input
    #[error("Cannot read input content from file '{1}'")]
    ReadFileInput(#[source] std::io::Error, PathBuf),
    /// Error while reading an input URL
    #[error(
        "Cannot read input content from URL: status code {0}. To check links in error pages, download and check locally instead."
    )]
    ReadInputUrlStatusCode(StatusCode),

    /// Error while reading stdin as input
    #[error("Cannot read content from stdin: {0}")]
    ReadStdinInput(#[from] std::io::Error),

    /// The given URI cannot be converted to a file path
    #[error("File not found. Check if file exists and path is correct")]
    InvalidFilePath(Uri),

    /// The given URI's fragment could not be found within the page content
    #[error("Cannot find fragment")]
    InvalidFragment(Uri),

    /// Cannot resolve local directory link using the configured index files
    #[error("Cannot find index file within directory")]
    InvalidIndexFile(Vec<String>),

    /// The given header could not be parsed.
    /// A possible error when converting a `HeaderValue` from a string or byte
    /// slice.
    #[error("Invalid HTTP header: {0}")]
    InvalidHeader(#[from] http::header::InvalidHeaderValue),

    /// The given input is neither a valid file path nor a valid URL
    #[error(
        "Input '{0}' not found as file and not a valid URL. Use full URL (e.g., https://example.com) or check file path."
    )]
    InvalidInput(String),

    /// Error while traversing an input directory
    #[error("Cannot traverse input directory: {0}")]
    DirTraversal(#[from] ignore::Error),

    /// The given glob pattern is not valid
    #[error("Invalid glob pattern: {0}")]
    InvalidGlobPattern(#[from] glob::PatternError),

    /// Used an insecure URI where a secure variant was reachable
    #[error("Insecure HTTP URL used, where '{0}' can be used instead")]
    InsecureURL(Uri),

    /// The given status code is invalid (not in range 100-999)
    #[error("Invalid status code: {0}")]
    InvalidStatusCode(u16),

    /// The given status code was not accepted (this depends on the `accept` configuration)
    #[error(
        "Rejected status code: {code} {reason}",
        code = .0.as_str(),
        reason = .0.canonical_reason().unwrap_or("Unknown status code")
    )]
    RejectedStatusCode(StatusCode),

    /// Regex error
    #[error("Regular expression error: {0}. Check regex syntax")]
    Regex(#[from] regex::Error),

    /// Basic authentication extractor error
    #[error("Basic authentication extraction error: {0}")]
    BasicAuthExtractorError(#[from] BasicAuthExtractorError),

    /// Status code selector parse error
    #[error("Unable to parse status code selector: {0}")]
    StatusCodeSelectorError(#[from] StatusCodeSelectorError),

    /// Error while parsing, validating, or resolving a URI
    #[error(transparent)]
    Uri(#[from] UriError),

    /// Error while resolving a wikilink against the base directory
    #[error(transparent)]
    Wikilink(#[from] WikilinkError),

    /// Error while running a preprocessor command
    #[error(transparent)]
    Preprocessor(#[from] PreprocessorError),

    /// Error while loading or saving cookies
    #[error(transparent)]
    Cookie(#[from] CookieError),

    /// Error while parsing or applying remap rules
    #[error(transparent)]
    Remap(#[from] RemapError),

    /// Error while checking a mail address
    #[error(transparent)]
    Mail(#[from] MailError),

    /// Internal, low-level error not meaningful as a link-level diagnostic
    #[error(transparent)]
    Internal(#[from] InternalError),
}

impl ErrorKind {
    /// Return more details about the given [`ErrorKind`]
    ///
    /// Which additional information we can extract depends on the underlying
    /// request type. The output is purely meant for humans (e.g. for status
    /// messages) and future changes are expected.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn details(&self) -> String {
        match self {
            ErrorKind::NetworkRequest(e) => utils::reqwest::analyze_error_chain(e),
            ErrorKind::Github(e) => e.details(),
            ErrorKind::ReadFileInput(e, path) => match e.kind() {
                std::io::ErrorKind::NotFound => "Check if file path is correct".to_string(),
                std::io::ErrorKind::PermissionDenied => format!(
                    "Permission denied: '{}'. Check file permissions",
                    path.display()
                ),
                std::io::ErrorKind::IsADirectory => format!(
                    "Path is a directory, not a file: '{}'. Check file path",
                    path.display()
                ),
                _ => format!("File read error for '{}': {e}", path.display()),
            },
            // This `details()` method never gets called for incorrect CLI
            // inputs, so whatever we put here, it won't be shown to the user.
            //
            // This returns an empty string as a sentinel value because it's handled as a
            // fatal application error rather than a link-level error.
            //
            // TODO: In the future, we should return an Option<String> or separate
            // application errors from library errors.
            ErrorKind::ReadInputUrlStatusCode(_) => String::new(),
            ErrorKind::Uri(e) => e.details(),
            ErrorKind::BuildRequestClient(_) => {
                format!("{self}: Check system configuration")
            }
            ErrorKind::InvalidHeader(_) => {
                format!("{self}. Check header format")
            }
            ErrorKind::Remap(e) => e.details(),
            ErrorKind::DirTraversal(_) => {
                format!("{self}. Check directory permissions")
            }
            ErrorKind::InvalidGlobPattern(_) => {
                format!("{self}. Check pattern syntax")
            }
            ErrorKind::InvalidStatusCode(_) => {
                format!("{self}. Must be in the range 100-999")
            }
            ErrorKind::BasicAuthExtractorError(_) => {
                format!("{self}. {}", "Check credentials format")
            }
            ErrorKind::Cookie(e) => e.details(),
            ErrorKind::Mail(e) => e.details(),
            ErrorKind::StatusCodeSelectorError(_) => {
                format!("{self}. Check 'accept' and 'cache_exclude_status' configuration")
            }
            ErrorKind::InvalidIndexFile(index_files) => {
                let details = match &index_files[..] {
                    [] => "Directory links are rejected because index_files is empty".into(),
                    [name] => format!("An index file ({name}) is required"),
                    [init @ .., tail] => format!(
                        "An index file ({}, or {}) is required",
                        init.join(", "),
                        tail
                    ),
                };

                format!("{self}: {details}")
            }
            ErrorKind::InvalidFragment(_)
            | ErrorKind::RejectedStatusCode(_)
            | ErrorKind::InvalidFilePath(_)
            | ErrorKind::InvalidInput(_)
            | ErrorKind::Regex(_)
            | ErrorKind::ReadResponseBody(_)
            | ErrorKind::Wikilink(_)
            | ErrorKind::Preprocessor(_)
            | ErrorKind::Internal(_)
            | ErrorKind::InsecureURL(_)
            | ErrorKind::ReadStdinInput(_) => self.to_string(),
        }
    }

    /// Return the underlying source of the given [`ErrorKind`]
    /// if it is a `reqwest::Error`.
    /// This is useful for extracting the status code of a failed request.
    /// If the error is not a `reqwest::Error`, `None` is returned.
    #[must_use]
    #[allow(clippy::redundant_closure_for_method_calls)]
    pub(crate) fn reqwest_error(&self) -> Option<&reqwest::Error> {
        self.source()
            .and_then(|e| e.downcast_ref::<reqwest::Error>())
    }

    /// Return the underlying source of the given [`ErrorKind`]
    /// if it is a `octocrab::Error`.
    /// This is useful for extracting the status code of a failed request.
    /// If the error is not a `octocrab::Error`, `None` is returned.
    #[must_use]
    #[allow(clippy::redundant_closure_for_method_calls)]
    pub(crate) fn github_error(&self) -> Option<&octocrab::Error> {
        self.source()
            .and_then(|e| e.downcast_ref::<octocrab::Error>())
    }
}

#[allow(clippy::match_same_arms)]
impl PartialEq for ErrorKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::NetworkRequest(e1), Self::NetworkRequest(e2)) => {
                e1.to_string() == e2.to_string()
            }
            (Self::ReadResponseBody(e1), Self::ReadResponseBody(e2)) => {
                e1.to_string() == e2.to_string()
            }
            (Self::BuildRequestClient(e1), Self::BuildRequestClient(e2)) => {
                e1.to_string() == e2.to_string()
            }
            (Self::ReadFileInput(e1, s1), Self::ReadFileInput(e2, s2)) => {
                e1.kind() == e2.kind() && s1 == s2
            }
            (Self::ReadInputUrlStatusCode(e1), Self::ReadInputUrlStatusCode(e2)) => e1 == e2,
            (Self::ReadStdinInput(e1), Self::ReadStdinInput(e2)) => e1.kind() == e2.kind(),
            (Self::Github(e1), Self::Github(e2)) => e1 == e2,
            (Self::Uri(e1), Self::Uri(e2)) => e1 == e2,
            (Self::Internal(e1), Self::Internal(e2)) => e1 == e2,
            (Self::InsecureURL(u1), Self::InsecureURL(u2)) => u1 == u2,
            (Self::InvalidGlobPattern(e1), Self::InvalidGlobPattern(e2)) => {
                e1.msg == e2.msg && e1.pos == e2.pos
            }
            (Self::InvalidHeader(_), Self::InvalidHeader(_)) => true,
            (Self::InvalidStatusCode(c1), Self::InvalidStatusCode(c2)) => c1 == c2,
            (Self::Regex(e1), Self::Regex(e2)) => e1.to_string() == e2.to_string(),
            (Self::DirTraversal(e1), Self::DirTraversal(e2)) => e1.to_string() == e2.to_string(),
            (Self::BasicAuthExtractorError(e1), Self::BasicAuthExtractorError(e2)) => {
                e1.to_string() == e2.to_string()
            }
            (Self::Cookie(e1), Self::Cookie(e2)) => e1 == e2,
            (Self::Remap(e1), Self::Remap(e2)) => e1 == e2,
            (Self::Mail(e1), Self::Mail(e2)) => e1 == e2,
            (Self::Wikilink(e1), Self::Wikilink(e2)) => e1 == e2,
            (Self::Preprocessor(e1), Self::Preprocessor(e2)) => e1 == e2,
            (Self::InvalidInput(s1), Self::InvalidInput(s2)) => s1 == s2,
            (Self::InvalidFilePath(u1), Self::InvalidFilePath(u2)) => u1 == u2,
            (Self::InvalidFragment(u1), Self::InvalidFragment(u2)) => u1 == u2,
            (Self::InvalidIndexFile(p1), Self::InvalidIndexFile(p2)) => p1 == p2,
            (Self::RejectedStatusCode(c1), Self::RejectedStatusCode(c2)) => c1 == c2,

            _ => false,
        }
    }
}

impl Eq for ErrorKind {}

#[allow(clippy::match_same_arms)]
impl Hash for ErrorKind {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        match self {
            Self::ReadFileInput(e, s) => (e.kind(), s).hash(state),
            Self::ReadInputUrlStatusCode(c) => c.hash(state),
            Self::ReadStdinInput(e) => e.kind().hash(state),
            Self::NetworkRequest(e) => e.to_string().hash(state),
            Self::ReadResponseBody(e) => e.to_string().hash(state),
            Self::BuildRequestClient(e) => e.to_string().hash(state),
            Self::Github(e) => e.hash(state),
            Self::Uri(e) => e.hash(state),
            Self::Internal(e) => e.hash(state),
            Self::DirTraversal(e) => e.to_string().hash(state),
            Self::InvalidInput(s) => s.hash(state),
            Self::InvalidFilePath(u) => u.hash(state),
            Self::InvalidFragment(u) => u.hash(state),
            Self::InvalidIndexFile(p) => p.hash(state),
            Self::InsecureURL(u, ..) => u.hash(state),
            Self::InvalidHeader(e) => e.to_string().hash(state),
            Self::InvalidGlobPattern(e) => e.to_string().hash(state),
            Self::InvalidStatusCode(c) => c.hash(state),
            Self::RejectedStatusCode(c) => c.hash(state),
            Self::Regex(e) => e.to_string().hash(state),
            Self::BasicAuthExtractorError(e) => e.to_string().hash(state),
            Self::Cookie(e) => e.hash(state),
            Self::Remap(e) => e.hash(state),
            Self::Mail(e) => e.hash(state),
            Self::StatusCodeSelectorError(e) => e.to_string().hash(state),
            Self::Preprocessor(e) => e.hash(state),
            Self::Wikilink(e) => e.hash(state),
        }
    }
}

impl Serialize for ErrorKind {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl From<Infallible> for ErrorKind {
    fn from(_: Infallible) -> Self {
        // tautological
        unreachable!()
    }
}

// The following conversions route low-level leaf errors through
// [`InternalError`] while preserving `?` ergonomics at call sites that produce
// the underlying error directly.
impl From<std::str::Utf8Error> for ErrorKind {
    fn from(e: std::str::Utf8Error) -> Self {
        ErrorKind::Internal(InternalError::Utf8(e))
    }
}

impl From<JoinError> for ErrorKind {
    fn from(e: JoinError) -> Self {
        ErrorKind::Internal(InternalError::RuntimeJoin(e))
    }
}

impl From<tokio::sync::mpsc::error::SendError<InputContent>> for ErrorKind {
    fn from(e: tokio::sync::mpsc::error::SendError<InputContent>) -> Self {
        ErrorKind::Internal(InternalError::Channel(e))
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    #[test]
    fn test_error_kind_details() {
        let status_error = ErrorKind::RejectedStatusCode(http::StatusCode::NOT_FOUND);
        assert!(status_error.to_string().contains("Not Found"));
    }
}
