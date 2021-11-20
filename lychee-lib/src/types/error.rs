use serde::{Serialize, Serializer};
use std::any::Any;
use std::hash::Hash;
use std::{convert::Infallible, path::PathBuf};
use thiserror::Error;

use crate::Uri;

/// Possible Errors when interacting with `lychee_lib`
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ErrorKind {
    // TODO: maybe needs to be split; currently first element is `Some` only for
    // reading files
    /// Any form of I/O error occurred while reading from a given path.
    #[error("Failed to read from path: `{}`, reason: {1}", match .0 {
        Some(p) => p.to_str().unwrap_or("<MALFORMED PATH>"),
        None => "<MALFORMED PATH>",
    })]
    IoError(Option<PathBuf>, std::io::Error),
    /// Errors which can occur when attempting to interpret a sequence of u8 as a string
    #[error("Attempted to interpret an invalid sequence of bytes as a string")]
    Utf8Error(#[from] std::str::Utf8Error),
    /// Reqwest network error
    #[error("Network error while trying to connect to an endpoint via reqwest")]
    ReqwestError(#[from] reqwest::Error),
    /// Hubcaps network error
    #[error("Network error when trying to connect to an endpoint via hubcaps")]
    HubcapsError(#[from] hubcaps::Error),
    /// The given string can not be parsed into a valid URL, e-mail address, or file path
    #[error("Cannot parse {0} as website url / file path or mail address: ({1:?})")]
    UrlParseError(String, (url::ParseError, Option<fast_chemail::ParseError>)),
    /// The given URI cannot be converted to a file path
    #[error("Cannot find file {0}")]
    InvalidFilePath(Uri),
    /// The given path cannot be converted to a URI
    #[error("Invalid path to URL conversion: {0}")]
    InvalidUrlFromPath(PathBuf),
    /// The given mail address is unreachable
    #[error("Unreachable mail address: {0}")]
    UnreachableEmailAddress(Uri),
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
    /// The given glob pattern is not valid
    #[error("UNIX glob pattern is invalid")]
    InvalidGlobPattern(#[from] glob::PatternError),
    /// The Github API could not be called because of a missing Github token.
    #[error("GitHub token not specified. To check GitHub links reliably, use `--github-token` flag / `GITHUB_TOKEN` env var.")]
    MissingGitHubToken,
    /// Used an insecure URI where a secure variant was reachable
    #[error("This URI is available in HTTPS protocol, but HTTP is provided, use '{0}' instead")]
    InsecureURL(Uri),
    /// An URL with an invalid host was found
    #[error("URL is missing a host")]
    InvalidUrlHost,
    /// Cannot parse the given URI
    #[error("The given URI is invalid: {0}")]
    InvalidURI(Uri),
}

impl PartialEq for ErrorKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::IoError(p1, e1), Self::IoError(p2, e2)) => p1 == p2 && e1.kind() == e2.kind(),
            (Self::ReqwestError(e1), Self::ReqwestError(e2)) => e1.to_string() == e2.to_string(),
            (Self::HubcapsError(e1), Self::HubcapsError(e2)) => e1.to_string() == e2.to_string(),
            (Self::UrlParseError(s1, e1), Self::UrlParseError(s2, e2)) => s1 == s2 && e1 == e2,
            (Self::UnreachableEmailAddress(u1), Self::UnreachableEmailAddress(u2))
            | (Self::InsecureURL(u1), Self::InsecureURL(u2)) => u1 == u2,
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

impl Hash for ErrorKind {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        match self {
            Self::IoError(p, e) => (p, e.kind()).hash(state),
            Self::ReqwestError(e) => e.to_string().hash(state),
            Self::HubcapsError(e) => e.to_string().hash(state),
            Self::FileNotFound(e) => e.to_string_lossy().hash(state),
            Self::UrlParseError(s, e) => (s, e.type_id()).hash(state),
            Self::InvalidURI(u) => u.hash(state),
            Self::InvalidUrlFromPath(p) => p.hash(state),
            Self::Utf8Error(e) => e.to_string().hash(state),
            Self::InvalidFilePath(u) | Self::UnreachableEmailAddress(u) | Self::InsecureURL(u) => {
                u.hash(state);
            }
            Self::InvalidBase(base, e) => (base, e).hash(state),
            Self::InvalidHeader(e) => e.to_string().hash(state),
            Self::InvalidGlobPattern(e) => e.to_string().hash(state),
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

impl From<(PathBuf, std::io::Error)> for ErrorKind {
    fn from(value: (PathBuf, std::io::Error)) -> Self {
        Self::IoError(Some(value.0), value.1)
    }
}

impl From<std::io::Error> for ErrorKind {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(None, e)
    }
}

impl From<tokio::task::JoinError> for ErrorKind {
    fn from(e: tokio::task::JoinError) -> Self {
        Self::IoError(None, e.into())
    }
}

impl From<url::ParseError> for ErrorKind {
    fn from(e: url::ParseError) -> Self {
        Self::UrlParseError("Cannot parse URL".to_string(), (e, None))
    }
}

impl From<(String, url::ParseError)> for ErrorKind {
    fn from(value: (String, url::ParseError)) -> Self {
        Self::UrlParseError(value.0, (value.1, None))
    }
}

impl From<(String, url::ParseError, fast_chemail::ParseError)> for ErrorKind {
    fn from(value: (String, url::ParseError, fast_chemail::ParseError)) -> Self {
        Self::UrlParseError(value.0, (value.1, Some(value.2)))
    }
}

impl From<Infallible> for ErrorKind {
    fn from(_: Infallible) -> Self {
        // tautological
        unreachable!()
    }
}
