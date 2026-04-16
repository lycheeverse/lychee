use http::StatusCode;
use serde::{Serialize, Serializer};
use std::error::Error;
use std::hash::Hash;
use std::{convert::Infallible, path::PathBuf};
use thiserror::Error;
use tokio::task::JoinError;

use super::InputContent;
use crate::types::StatusCodeSelectorError;
use crate::{Uri, basic_auth::BasicAuthExtractorError, utils};

/// Kinds of status errors
/// Note: The error messages can change over time, so don't match on the output
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

    /// Network error while using GitHub API
    #[error("Network error while using GitHub client")]
    GithubRequest(#[from] Box<octocrab::Error>),

    /// Error while executing a future on the Tokio runtime
    #[error("Task failed to execute to completion: {0}")]
    RuntimeJoin(#[from] JoinError),

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

    /// Errors which can occur when attempting to interpret a sequence of u8 as a string
    ///
    #[error(
        "Encountered invalid UTF-8 sequence, while trying to interpret bytes UTF-8 string: {0}"
    )]
    Utf8(#[from] std::str::Utf8Error),

    /// The GitHub client required for making requests cannot be created
    #[error("Failed to create GitHub client")]
    BuildGithubClient(#[source] Box<octocrab::Error>),

    /// Invalid GitHub URL
    #[error("GitHub URL is invalid: {0}")]
    InvalidGithubUrl(String),

    /// The input is empty and not accepted as a valid URL
    #[error("Empty URL found but a URL must not be empty")]
    EmptyUrl,

    /// The given string can not be parsed into a valid URL, e-mail address, or file path
    #[error("Cannot parse '{1}' into a URL: {0}")]
    ParseUrl(#[source] url::ParseError, String),

    /// The given string is a root-relative link and cannot be parsed without a known root-dir
    #[error("Cannot resolve root-relative link '{0}'")]
    RootRelativeLinkWithoutRoot(String),

    /// The given URI cannot be converted to a file path
    #[error("File not found. Check if file exists and path is correct")]
    InvalidFilePath(Uri),

    /// The given URI's fragment could not be found within the page content
    #[error("Cannot find fragment")]
    InvalidFragment(Uri),

    /// Cannot resolve local directory link using the configured index files
    #[error("Cannot find index file within directory")]
    InvalidIndexFile(Vec<String>),

    /// The given path cannot be converted to a URI
    #[error("Cannot convert path to URL: '{0}'")]
    InvalidUrlFromPath(PathBuf),

    /// The given mail address is unreachable
    #[error("Unreachable mail address {0}")]
    UnreachableEmailAddress(Uri, String),

    /// The given header could not be parsed.
    /// A possible error when converting a `HeaderValue` from a string or byte
    /// slice.
    #[error("Invalid HTTP header: {0}")]
    InvalidHeader(#[from] http::header::InvalidHeaderValue),

    /// The given string can not be parsed into a valid base URL or base directory
    #[error("Invalid base URL or directory: '{0}'. {1}")]
    InvalidBase(String, String),

    /// The given URI type is not supported
    #[error("Unsupported URI type: '{0}'")]
    UnsupportedUriType(String),

    /// The given input can not be parsed into a valid URI remap
    #[error("Invalid remap pattern: {0}")]
    InvalidUrlRemap(String),

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

    /// The GitHub API could not be called because of a missing GitHub token.
    #[error("GitHub token required")]
    MissingGitHubToken,

    /// Used an insecure URI where a secure variant was reachable
    #[error("Insecure HTTP URL used, where '{0}' can be used instead")]
    InsecureURL(Uri),

    /// Error while sending/receiving messages from MPSC channel
    #[error("Internal communication error, cannot send/receive message over channel: {0}")]
    Channel(#[from] tokio::sync::mpsc::error::SendError<InputContent>),

    /// A URL without a host was found
    #[error("URL is missing a hostname")]
    InvalidUrlHost,

    /// Cannot parse the given URI
    #[error("The given URI is invalid, check URI syntax: {0}")]
    InvalidURI(Uri),

    /// The given status code is invalid (not in range 100-999)
    #[error("Invalid status code: {0}")]
    InvalidStatusCode(u16),

    /// The given status code was not accepted (this depends on the `accept` configuration)
    #[error(
        r#"Rejected status code: {code} {reason} (configurable with "accept" option)"#,
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

    /// Cannot handle cookies
    #[error("Cookie handling error: {0}")]
    Cookies(String),

    /// Status code selector parse error
    #[error("Unable to parse status code selector: {0}")]
    StatusCodeSelectorError(#[from] StatusCodeSelectorError),

    /// Preprocessor command error
    #[error("Preprocessor command '{command}' failed with '{reason}'")]
    PreprocessorError {
        /// The command which did not execute successfully
        command: String,
        /// The reason the command failed
        reason: String,
    },

    /// The extracted `WikiLink` could not be found by searching the directory
    #[error("Wikilink {0} not found at {1}")]
    WikilinkNotFound(Uri, PathBuf),

    /// Invalid base URL for `WikiLink` checking
    #[error("Invalid base URL for WikiLink checking: {0}")]
    WikilinkInvalidBase(String),
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
            ErrorKind::GithubRequest(e) => {
                let detail = if let octocrab::Error::GitHub { source, .. } = &**e {
                    source.message.clone()
                } else {
                    e.to_string()
                };
                format!("{self}: {detail}")
            }
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
            ErrorKind::ParseUrl(e, _url) => {
                let detail = match e {
                    url::ParseError::RelativeUrlWithoutBase => {
                        ": This relative link was found inside an input source that has no base location"
                    }
                    _ => "",
                };

                format!("{self}{detail}")
            }
            ErrorKind::RootRelativeLinkWithoutRoot(_) => {
                format!("{self}: To resolve root-relative links in local files, provide a root dir")
            }
            ErrorKind::BuildRequestClient(_) => {
                format!("{self}: Check system configuration")
            }
            ErrorKind::BuildGithubClient(error) => {
                format!("{self}: {error}. Check token and network connectivity")
            }
            ErrorKind::InvalidGithubUrl(_) => {
                format!("{self}. Check URL syntax")
            }
            ErrorKind::InvalidUrlFromPath(_) => {
                format!("{self}. Check path format")
            }
            ErrorKind::UnreachableEmailAddress(_uri, reason) => reason.clone(),
            ErrorKind::InvalidHeader(_) => {
                format!("{self}. Check header format")
            }
            ErrorKind::UnsupportedUriType(_) => {
                format!("{self}. Only http, https, file, and mailto are supported")
            }
            ErrorKind::InvalidUrlRemap(_) => {
                format!("{self}. Check remap syntax")
            }
            ErrorKind::DirTraversal(_) => {
                format!("{self}. Check directory permissions")
            }
            ErrorKind::InvalidGlobPattern(_) => {
                format!("{self}. Check pattern syntax")
            }
            ErrorKind::MissingGitHubToken => {
                format!("{self}. Use --github-token flag or GITHUB_TOKEN environment variable")
            }
            ErrorKind::InvalidStatusCode(_) => {
                format!("{self}. Must be in the range 100-999")
            }
            ErrorKind::BasicAuthExtractorError(_) => {
                format!("{self}. {}", "Check credentials format")
            }
            ErrorKind::Cookies(_) => {
                format!("{self}. Check cookie file format")
            }
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
            | ErrorKind::InvalidURI(_)
            | ErrorKind::InvalidInput(_)
            | ErrorKind::Regex(_)
            | ErrorKind::Utf8(_)
            | ErrorKind::ReadResponseBody(_)
            | ErrorKind::RuntimeJoin(_)
            | ErrorKind::WikilinkInvalidBase(_)
            | ErrorKind::Channel(_)
            | ErrorKind::InsecureURL(_)
            | ErrorKind::ReadStdinInput(_)
            | ErrorKind::InvalidBase(_, _)
            | ErrorKind::WikilinkNotFound(_, _)
            | ErrorKind::EmptyUrl
            | ErrorKind::InvalidUrlHost
            | ErrorKind::PreprocessorError {
                command: _,
                reason: _,
            } => self.to_string(),
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
            (Self::RuntimeJoin(e1), Self::RuntimeJoin(e2)) => e1.to_string() == e2.to_string(),
            (Self::ReadFileInput(e1, s1), Self::ReadFileInput(e2, s2)) => {
                e1.kind() == e2.kind() && s1 == s2
            }
            (Self::ReadInputUrlStatusCode(e1), Self::ReadInputUrlStatusCode(e2)) => e1 == e2,
            (Self::ReadStdinInput(e1), Self::ReadStdinInput(e2)) => e1.kind() == e2.kind(),
            (Self::GithubRequest(e1), Self::GithubRequest(e2)) => e1.to_string() == e2.to_string(),
            (Self::InvalidGithubUrl(s1), Self::InvalidGithubUrl(s2)) => s1 == s2,
            (Self::ParseUrl(s1, e1), Self::ParseUrl(s2, e2)) => s1 == s2 && e1 == e2,
            (Self::UnreachableEmailAddress(u1, ..), Self::UnreachableEmailAddress(u2, ..)) => {
                u1 == u2
            }
            (Self::InsecureURL(u1), Self::InsecureURL(u2)) => u1 == u2,
            (Self::InvalidGlobPattern(e1), Self::InvalidGlobPattern(e2)) => {
                e1.msg == e2.msg && e1.pos == e2.pos
            }
            (Self::InvalidHeader(_), Self::InvalidHeader(_))
            | (Self::MissingGitHubToken, Self::MissingGitHubToken) => true,
            (Self::InvalidStatusCode(c1), Self::InvalidStatusCode(c2)) => c1 == c2,
            (Self::InvalidUrlHost, Self::InvalidUrlHost) => true,
            (Self::InvalidURI(u1), Self::InvalidURI(u2)) => u1 == u2,
            (Self::Regex(e1), Self::Regex(e2)) => e1.to_string() == e2.to_string(),
            (Self::DirTraversal(e1), Self::DirTraversal(e2)) => e1.to_string() == e2.to_string(),
            (Self::Channel(_), Self::Channel(_)) => true,
            (Self::BasicAuthExtractorError(e1), Self::BasicAuthExtractorError(e2)) => {
                e1.to_string() == e2.to_string()
            }
            (Self::Cookies(e1), Self::Cookies(e2)) => e1 == e2,
            (Self::InvalidInput(s1), Self::InvalidInput(s2)) => s1 == s2,
            (Self::InvalidFilePath(u1), Self::InvalidFilePath(u2)) => u1 == u2,
            (Self::InvalidFragment(u1), Self::InvalidFragment(u2)) => u1 == u2,
            (Self::InvalidIndexFile(p1), Self::InvalidIndexFile(p2)) => p1 == p2,
            (Self::InvalidUrlFromPath(p1), Self::InvalidUrlFromPath(p2)) => p1 == p2,
            (Self::InvalidBase(b1, e1), Self::InvalidBase(b2, e2)) => b1 == b2 && e1 == e2,
            (Self::InvalidUrlRemap(r1), Self::InvalidUrlRemap(r2)) => r1 == r2,
            (Self::EmptyUrl, Self::EmptyUrl) => true,
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
            Self::RuntimeJoin(e) => e.to_string().hash(state),
            Self::ReadFileInput(e, s) => (e.kind(), s).hash(state),
            Self::ReadInputUrlStatusCode(c) => c.hash(state),
            Self::ReadStdinInput(e) => e.kind().hash(state),
            Self::NetworkRequest(e) => e.to_string().hash(state),
            Self::ReadResponseBody(e) => e.to_string().hash(state),
            Self::BuildRequestClient(e) => e.to_string().hash(state),
            Self::BuildGithubClient(e) => e.to_string().hash(state),
            Self::GithubRequest(e) => e.to_string().hash(state),
            Self::InvalidGithubUrl(s) => s.hash(state),
            Self::DirTraversal(e) => e.to_string().hash(state),
            Self::InvalidInput(s) => s.hash(state),
            Self::EmptyUrl => "Empty URL".hash(state),
            Self::ParseUrl(e, s) => (e.to_string(), s).hash(state),
            Self::RootRelativeLinkWithoutRoot(s) => s.hash(state),
            Self::InvalidURI(u) => u.hash(state),
            Self::InvalidUrlFromPath(p) => p.hash(state),
            Self::Utf8(e) => e.to_string().hash(state),
            Self::InvalidFilePath(u) => u.hash(state),
            Self::InvalidFragment(u) => u.hash(state),
            Self::InvalidIndexFile(p) => p.hash(state),
            Self::UnreachableEmailAddress(u, ..) => u.hash(state),
            Self::InsecureURL(u, ..) => u.hash(state),
            Self::InvalidBase(base, e) => (base, e).hash(state),
            Self::UnsupportedUriType(s) => s.hash(state),
            Self::InvalidUrlRemap(remap) => (remap).hash(state),
            Self::InvalidHeader(e) => e.to_string().hash(state),
            Self::InvalidGlobPattern(e) => e.to_string().hash(state),
            Self::InvalidStatusCode(c) => c.hash(state),
            Self::RejectedStatusCode(c) => c.hash(state),
            Self::Channel(e) => e.to_string().hash(state),
            Self::MissingGitHubToken | Self::InvalidUrlHost => {
                std::mem::discriminant(self).hash(state);
            }
            Self::Regex(e) => e.to_string().hash(state),
            Self::BasicAuthExtractorError(e) => e.to_string().hash(state),
            Self::Cookies(e) => e.hash(state),
            Self::StatusCodeSelectorError(e) => e.to_string().hash(state),
            Self::PreprocessorError { command, reason } => (command, reason).hash(state),
            Self::WikilinkNotFound(uri, pathbuf) => (uri, pathbuf).hash(state),
            Self::WikilinkInvalidBase(e) => e.hash(state),
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

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    #[test]
    fn test_error_kind_details() {
        // Test rejected status code
        let status_error = ErrorKind::RejectedStatusCode(http::StatusCode::NOT_FOUND);
        assert!(status_error.to_string().contains("Not Found"));

        // Test redirected status code
        let redir_error = ErrorKind::RejectedStatusCode(http::StatusCode::MOVED_PERMANENTLY);
        assert!(
            redir_error
                .details()
                .contains(r#"(configurable with "accept" option)"#)
        );
    }
}
