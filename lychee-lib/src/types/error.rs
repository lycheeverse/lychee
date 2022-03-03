use serde::{Serialize, Serializer};
use std::any::Any;
use std::hash::Hash;
use std::{convert::Infallible, path::PathBuf};
use thiserror::Error;
use tokio::task::JoinError;

use super::InputContent;
use crate::Uri;

/// Kinds of status errors
/// Note: The error messages can change over time, so don't match on the output
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ErrorKind {
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
    /// Network error while handling request
    #[error("Network error")]
    NetworkRequest(#[source] reqwest::Error),
    /// Cannot read the body of the received response
    #[error("Error reading response body")]
    ReadResponseBody(#[source] reqwest::Error),
    /// The network client required for making requests cannot be created
    #[error("Error creating request client")]
    BuildRequestClient(#[source] reqwest::Error),
    /// The Github client required for making requests cannot be created
    #[error("Error creating Github client")]
    BuildGithubClient(#[source] octocrab::Error),
    /// Network error while using Github API
    #[error("Network error (GitHub client)")]
    GithubRequest(#[from] octocrab::Error),
    /// Invalid Github URL
    #[error("Github URL is invalid: {0}")]
    InvalidGithubUrl(String),
    /// The given string can not be parsed into a valid URL, e-mail address, or file path
    #[error("Cannot parse string `{1}` as website url")]
    ParseUrl(#[source] url::ParseError, String),
    /// The given URI cannot be converted to a file path
    #[error("Cannot find file")]
    InvalidFilePath(Uri),
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
    /// The given path does not resolve to a valid file
    #[error("Cannot find local file {0}")]
    FileNotFound(PathBuf),
    /// Error while traversing an input directory
    #[error("Cannot traverse input directory")]
    DirTraversal(#[from] jwalk::Error),
    /// The given glob pattern is not valid
    #[error("UNIX glob pattern is invalid")]
    InvalidGlobPattern(#[from] glob::PatternError),
    /// The Github API could not be called because of a missing Github token.
    #[error("GitHub token not specified. To check GitHub links reliably, use `--github-token` flag / `GITHUB_TOKEN` env var.")]
    MissingGitHubToken,
    /// Used an insecure URI where a secure variant was reachable
    #[error("This URI is available in HTTPS protocol, but HTTP is provided, use '{0}' instead")]
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
            Self::GithubRequest(e) => e.type_id().hash(state),
            Self::InvalidGithubUrl(s) => s.hash(state),
            Self::DirTraversal(e) => e.to_string().hash(state),
            Self::FileNotFound(e) => e.to_string_lossy().hash(state),
            Self::ParseUrl(e, s) => (e.type_id(), s).hash(state),
            Self::InvalidURI(u) => u.hash(state),
            Self::InvalidUrlFromPath(p) => p.hash(state),
            Self::Utf8(e) => e.to_string().hash(state),
            Self::InvalidFilePath(u) => u.hash(state),
            Self::UnreachableEmailAddress(u, ..) => u.hash(state),
            Self::InsecureURL(u, ..) => u.hash(state),
            Self::InvalidBase(base, e) => (base, e).hash(state),
            Self::InvalidHeader(e) => e.to_string().hash(state),
            Self::InvalidGlobPattern(e) => e.to_string().hash(state),
            Self::Channel(e) => e.to_string().hash(state),
            Self::MissingGitHubToken | Self::InvalidUrlHost => {
                std::mem::discriminant(self).hash(state);
            }
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
