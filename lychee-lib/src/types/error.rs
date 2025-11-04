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
    #[error("Network error")]
    NetworkRequest(#[source] reqwest::Error),
    /// Cannot read the body of the received response
    #[error("Error reading response body: {0}")]
    ReadResponseBody(#[source] reqwest::Error),
    /// The network client required for making requests cannot be created
    #[error("Error creating request client: {0}")]
    BuildRequestClient(#[source] reqwest::Error),

    /// Network error while using GitHub API
    #[error("Network error (GitHub client)")]
    GithubRequest(#[from] Box<octocrab::Error>),

    /// Error while executing a future on the Tokio runtime
    #[error("Task failed to execute to completion")]
    RuntimeJoin(#[from] JoinError),

    /// Error while converting a file to an input
    #[error("Cannot read input content from file `{1}`")]
    ReadFileInput(#[source] std::io::Error, PathBuf),

    /// Error while reading stdin as input
    #[error("Cannot read input content from stdin")]
    ReadStdinInput(#[from] std::io::Error),

    /// Errors which can occur when attempting to interpret a sequence of u8 as a string
    #[error("Attempted to interpret an invalid sequence of bytes as a string")]
    Utf8(#[from] std::str::Utf8Error),

    /// The GitHub client required for making requests cannot be created
    #[error("Error creating GitHub client")]
    BuildGithubClient(#[source] Box<octocrab::Error>),

    /// Invalid GitHub URL
    #[error("GitHub URL is invalid: {0}")]
    InvalidGithubUrl(String),

    /// The input is empty and not accepted as a valid URL
    #[error("URL cannot be empty")]
    EmptyUrl,

    /// The given string can not be parsed into a valid URL, e-mail address, or file path
    #[error("Cannot parse string `{1}` as website url: {0}")]
    ParseUrl(#[source] url::ParseError, String),

    /// The given URI cannot be converted to a file path
    #[error("Cannot find file")]
    InvalidFilePath(Uri),

    /// The given URI's fragment could not be found within the page content
    #[error("Cannot find fragment")]
    InvalidFragment(Uri),

    /// Cannot resolve local directory link using the configured index files
    #[error("Cannot find index file within directory")]
    InvalidIndexFile(Vec<String>),

    /// The given path cannot be converted to a URI
    #[error("Invalid path to URL conversion: {0}")]
    InvalidUrlFromPath(PathBuf),

    /// The given mail address is unreachable
    #[error("Unreachable mail address: {0}: {1}")]
    UnreachableEmailAddress(Uri, String),

    /// The given header could not be parsed.
    /// A possible error when converting a `HeaderValue` from a string or byte
    /// slice.
    #[error("Header could not be parsed.")]
    InvalidHeader(#[from] http::header::InvalidHeaderValue),

    /// The given string can not be parsed into a valid base URL or base directory
    #[error("Error with base dir `{0}` : {1}")]
    InvalidBase(String, String),

    /// Cannot join the given text with the base URL
    #[error("Cannot join '{0}' with the base URL")]
    InvalidBaseJoin(String),

    /// Cannot convert the given path to a URI
    #[error("Cannot convert path '{0}' to a URI")]
    InvalidPathToUri(String),

    /// Root dir must be an absolute path
    #[error("Root dir must be an absolute path: '{0}'")]
    RootDirMustBeAbsolute(PathBuf),

    /// The given URI type is not supported
    #[error("Unsupported URI type: '{0}'")]
    UnsupportedUriType(String),

    /// The given input can not be parsed into a valid URI remapping
    #[error("Error remapping URL: `{0}`")]
    InvalidUrlRemap(String),

    /// The given path does not resolve to a valid file
    #[error("Invalid file path: {0}")]
    InvalidFile(PathBuf),

    /// Error while traversing an input directory
    #[error("Cannot traverse input directory: {0}")]
    DirTraversal(#[from] ignore::Error),

    /// The given glob pattern is not valid
    #[error("UNIX glob pattern is invalid")]
    InvalidGlobPattern(#[from] glob::PatternError),

    /// The GitHub API could not be called because of a missing GitHub token.
    #[error(
        "GitHub token not specified. To check GitHub links reliably, use `--github-token` flag / `GITHUB_TOKEN` env var."
    )]
    MissingGitHubToken,

    /// Used an insecure URI where a secure variant was reachable
    #[error("This URI is available in HTTPS protocol, but HTTP is provided. Use '{0}' instead")]
    InsecureURL(Uri),

    /// Error while sending/receiving messages from MPSC channel
    #[error("Cannot send/receive message from channel")]
    Channel(#[from] tokio::sync::mpsc::error::SendError<InputContent>),

    /// An URL with an invalid host was found
    #[error("URL is missing a host")]
    InvalidUrlHost,

    /// Cannot parse the given URI
    #[error("The given URI is invalid: {0}")]
    InvalidURI(Uri),

    /// The given status code is invalid (not in the range 100-1000)
    #[error("Invalid status code: {0}")]
    InvalidStatusCode(u16),

    /// The given status code was not accepted (this depends on the `accept` configuration)
    #[error(r#"Rejected status code (this depends on your "accept" configuration)"#)]
    RejectedStatusCode(StatusCode),

    /// Regex error
    #[error("Error when using regex engine: {0}")]
    Regex(#[from] regex::Error),

    /// Basic auth extractor error
    #[error("Basic auth extractor error")]
    BasicAuthExtractorError(#[from] BasicAuthExtractorError),

    /// Cannot load cookies
    #[error("Cannot load cookies")]
    Cookies(String),

    /// Status code selector parse error
    #[error("Status code range error")]
    StatusCodeSelectorError(#[from] StatusCodeSelectorError),

    /// Preprocessor command error
    #[error("Preprocessor command '{command}' failed: {reason}")]
    PreprocessorError {
        /// The command which did not execute successfully
        command: String,
        /// The reason the command failed
        reason: String,
    },
}

impl ErrorKind {
    /// Return more details about the given [`ErrorKind`]
    ///
    /// Which additional information we can extract depends on the underlying
    /// request type. The output is purely meant for humans (e.g. for status
    /// messages) and future changes are expected.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn details(&self) -> Option<String> {
        match self {
            ErrorKind::NetworkRequest(e) => {
                        // Get detailed, actionable error analysis
                        Some(utils::reqwest::analyze_error_chain(e))
                    }
            ErrorKind::RejectedStatusCode(status) => Some(
                        status
                            .canonical_reason()
                            .unwrap_or("Unknown status code")
                            .to_string(),
                    ),
            ErrorKind::GithubRequest(e) => {
                        if let octocrab::Error::GitHub { source, .. } = &**e {
                            Some(source.message.clone())
                        } else {
                            // Fall back to generic error analysis
                            Some(e.to_string())
                        }
                    }
            ErrorKind::InvalidFilePath(_uri) => Some(
                "File not found. Check if file exists and path is correct".to_string()
            ),
            ErrorKind::ReadFileInput(e, path) => match e.kind() {
                        std::io::ErrorKind::NotFound => Some(
                            "Check if file path is correct".to_string()
                        ),
                        std::io::ErrorKind::PermissionDenied => Some(format!(
                            "Permission denied: '{}'. Check file permissions",
                            path.display()
                        )),
                        std::io::ErrorKind::IsADirectory => Some(format!(
                            "Path is a directory, not a file: '{}'. Check file path",
                            path.display()
                        )),
                        _ => Some(format!("File read error for '{}': {}", path.display(), e)),
                    },
            ErrorKind::ReadStdinInput(e) => match e.kind() {
                        std::io::ErrorKind::UnexpectedEof => {
                            Some("Stdin input ended unexpectedly. Check input data".to_string())
                        }
                        std::io::ErrorKind::InvalidData => {
                            Some("Invalid data from stdin. Check input format".to_string())
                        }
                        _ => Some(format!("Stdin read error: {e}")),
                    },
            ErrorKind::ParseUrl(_, url) => {
                        Some(format!("Invalid URL format: '{url}'. Check URL syntax"))
                    }
            ErrorKind::EmptyUrl => {
                        Some("Empty URL found. Check for missing links or malformed markdown".to_string())
                    }
            ErrorKind::InvalidFile(path) => Some(format!(
                        "Invalid file path: '{}'. Check if file exists and is readable",
                        path.display()
                    )),
            ErrorKind::ReadResponseBody(error) => Some(format!(
                "Failed to read response body: {error}. Server may have sent invalid data",
            )),
            ErrorKind::BuildRequestClient(error) => Some(format!(
                "Failed to create HTTP client: {error}. Check system configuration",
            )),
            ErrorKind::RuntimeJoin(join_error) => Some(format!(
                "Task execution failed: {join_error}. Internal processing error"
            )),
            ErrorKind::Utf8(_utf8_error) => Some(
                "Invalid UTF-8 sequence found. File contains non-UTF-8 characters".to_string()
            ),
            ErrorKind::BuildGithubClient(error) => Some(format!(
                "Failed to create GitHub client: {error}. Check token and network connectivity",
            )),
            ErrorKind::InvalidGithubUrl(url) => Some(format!(
                "Invalid GitHub URL format: '{url}'. Check URL syntax",
            )),
            ErrorKind::InvalidFragment(_uri) => Some(
                "Fragment not found in document. Check if fragment exists or page structure".to_string()
            ),
            ErrorKind::InvalidUrlFromPath(path_buf) => Some(format!(
                "Cannot convert path to URL: '{}'. Check path format",
                path_buf.display()
            )),
            ErrorKind::UnreachableEmailAddress(uri, reason) => Some(format!(
                "Email address unreachable: '{uri}'. {reason}",
            )),
            ErrorKind::InvalidHeader(invalid_header_value) => Some(format!(
                "Invalid HTTP header: {invalid_header_value}. Check header format",
            )),
            ErrorKind::InvalidBase(base, reason) => Some(format!(
                "Invalid base URL or directory: '{base}'. {reason}",
            )),
            ErrorKind::InvalidBaseJoin(text) => Some(format!(
                "Cannot join '{text}' with base URL. Check relative path format",
            )),
            ErrorKind::InvalidPathToUri(path) => Some(format!(
                "Cannot convert path to URI: '{path}'. Check path format",
            )),
            ErrorKind::RootDirMustBeAbsolute(path_buf) => Some(format!(
                "Root directory must be absolute: '{}'. Use full path",
                path_buf.display()
            )),
            ErrorKind::UnsupportedUriType(uri_type) => Some(format!(
                "Unsupported URI type: '{uri_type}'. Only http, https, file, and mailto are supported",
            )),
            ErrorKind::InvalidUrlRemap(remap) => Some(format!(
                "Invalid URL remapping: '{remap}'. Check remapping syntax",
            )),
            ErrorKind::DirTraversal(error) => Some(format!(
                "Directory traversal failed: {error}. Check directory permissions",
            )),
            ErrorKind::InvalidGlobPattern(pattern_error) => Some(format!(
                "Invalid glob pattern: {pattern_error}. Check pattern syntax",
            )),
            ErrorKind::MissingGitHubToken => Some(
                "GitHub token required. Use --github-token flag or GITHUB_TOKEN environment variable".to_string()
            ),
            ErrorKind::InsecureURL(uri) => Some(format!(
                "Insecure HTTP URL detected: use '{}' instead of HTTP",
                uri.as_str().replace("http://", "https://")
            )),
            ErrorKind::Channel(_send_error) => Some(
                "Internal communication error. Processing thread failed".to_string()
            ),
            ErrorKind::InvalidUrlHost => Some(
                "URL missing hostname. Check URL format".to_string()
            ),
            ErrorKind::InvalidURI(uri) => Some(format!(
                "Invalid URI format: '{uri}'. Check URI syntax",
            )),
            ErrorKind::InvalidStatusCode(code) => Some(format!(
                "Invalid HTTP status code: {code}. Must be between 100-999",
            )),
            ErrorKind::Regex(error) => Some(format!(
                "Regular expression error: {error}. Check regex syntax",
            )),
            ErrorKind::BasicAuthExtractorError(basic_auth_extractor_error) => Some(format!(
                "Basic authentication error: {basic_auth_extractor_error}. Check credentials format",
            )),
            ErrorKind::Cookies(reason) => Some(format!(
                "Cookie handling error: {reason}. Check cookie file format",
            )),
            ErrorKind::StatusCodeSelectorError(status_code_selector_error) => Some(format!(
                "Status code selector error: {status_code_selector_error}. Check accept configuration",
            )),
            ErrorKind::InvalidIndexFile(index_files) => match &index_files[..] {
                [] => "No directory links are allowed because index_files is defined and empty".to_string(),
                [name] => format!("An index file ({name}) is required"),
                [init @ .., tail] => format!("An index file ({}, or {}) is required", init.join(", "), tail),
            }.into(),
            ErrorKind::PreprocessorError{command, reason} => Some(format!("Command '{command}' failed {reason}. Check value of the pre option"))
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
            (Self::InvalidFile(p1), Self::InvalidFile(p2)) => p1 == p2,
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
            Self::ReadStdinInput(e) => e.kind().hash(state),
            Self::NetworkRequest(e) => e.to_string().hash(state),
            Self::ReadResponseBody(e) => e.to_string().hash(state),
            Self::BuildRequestClient(e) => e.to_string().hash(state),
            Self::BuildGithubClient(e) => e.to_string().hash(state),
            Self::GithubRequest(e) => e.to_string().hash(state),
            Self::InvalidGithubUrl(s) => s.hash(state),
            Self::DirTraversal(e) => e.to_string().hash(state),
            Self::InvalidFile(e) => e.to_string_lossy().hash(state),
            Self::EmptyUrl => "Empty URL".hash(state),
            Self::ParseUrl(e, s) => (e.to_string(), s).hash(state),
            Self::InvalidURI(u) => u.hash(state),
            Self::InvalidUrlFromPath(p) => p.hash(state),
            Self::Utf8(e) => e.to_string().hash(state),
            Self::InvalidFilePath(u) => u.hash(state),
            Self::InvalidFragment(u) => u.hash(state),
            Self::InvalidIndexFile(p) => p.hash(state),
            Self::UnreachableEmailAddress(u, ..) => u.hash(state),
            Self::InsecureURL(u, ..) => u.hash(state),
            Self::InvalidBase(base, e) => (base, e).hash(state),
            Self::InvalidBaseJoin(s) => s.hash(state),
            Self::InvalidPathToUri(s) => s.hash(state),
            Self::RootDirMustBeAbsolute(s) => s.hash(state),
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
