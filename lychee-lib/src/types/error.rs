use std::{any::Any, convert::Infallible, fmt::Display, hash::Hash, path::PathBuf};

use http::header::InvalidHeaderValue;
use serde::{Serialize, Serializer};

use crate::Uri;

/// Kinds of status errors.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Any form of I/O error occurred while reading from a given path
    // TODO: maybe needs to be split; currently first element is `Some` only for
    // reading files
    IoError(Option<PathBuf>, std::io::Error),
    /// Network error when trying to connect to an endpoint via reqwest
    ReqwestError(reqwest::Error),
    /// Network error when trying to connect to an endpoint via hubcaps
    HubcapsError(hubcaps::Error),
    /// The given string can not be parsed into a valid URL, e-mail address, or file path
    UrlParseError(String, (url::ParseError, Option<fast_chemail::ParseError>)),
    /// The given URI cannot be converted to a file path
    InvalidFileUri(Uri),
    /// The given path cannot be converted to a URI
    InvalidPath(PathBuf),
    /// The given mail address is unreachable
    UnreachableEmailAddress(Uri),
    /// The given header could not be parsed.
    /// A possible error when converting a `HeaderValue` from a string or byte
    /// slice.
    InvalidHeader(InvalidHeaderValue),
    /// The given string can not be parsed into a valid base URL or base directory
    InvalidBase(String, String),
    /// Cannot find local file
    FileNotFound(PathBuf),
    /// The given UNIX glob pattern is invalid
    InvalidGlobPattern(glob::PatternError),
    /// The Github API could not be called because of a missing Github token
    MissingGitHubToken,
}

impl PartialEq for ErrorKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::IoError(p1, e1), Self::IoError(p2, e2)) => p1 == p2 && e1.kind() == e2.kind(),
            (Self::ReqwestError(e1), Self::ReqwestError(e2)) => e1.to_string() == e2.to_string(),
            (Self::HubcapsError(e1), Self::HubcapsError(e2)) => e1.to_string() == e2.to_string(),
            (Self::UrlParseError(s1, e1), Self::UrlParseError(s2, e2)) => s1 == s2 && e1 == e2,
            (Self::UnreachableEmailAddress(u1), Self::UnreachableEmailAddress(u2)) => u1 == u2,
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
            Self::InvalidFileUri(f) => f.hash(state),
            Self::InvalidPath(p) => p.hash(state),
            Self::UnreachableEmailAddress(u) => u.hash(state),
            Self::InvalidHeader(e) => e.to_string().hash(state),
            Self::InvalidGlobPattern(e) => e.to_string().hash(state),
            Self::MissingGitHubToken => std::mem::discriminant(self).hash(state),
            ErrorKind::InvalidBase(base, e) => (base, e).hash(state),
        }
    }
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(Some(p), e) => write!(
                f,
                "Failed to read file: `{}`, reason: {}",
                p.to_str().unwrap_or("<MALFORMED PATH>"),
                e
            ),
            Self::IoError(None, e) => e.fmt(f),
            Self::ReqwestError(e) => e.fmt(f),
            Self::HubcapsError(e) => e.fmt(f),
            Self::FileNotFound(e) => write!(f, "{}", e.to_string_lossy()),
            Self::UrlParseError(s, (url_err, Some(mail_err))) => {
                write!(
                    f,
                    "Cannot parse {} as website url ({}) or mail address ({})",
                    s, url_err, mail_err
                )
            }
            Self::UrlParseError(s, (url_err, None)) => {
                write!(f, "Cannot parse {} as website url ({})", s, url_err)
            }
            Self::InvalidFileUri(u) => write!(f, "Invalid file URI: {}", u),
            Self::InvalidPath(p) => write!(f, "Invalid path: {}", p.display()),
            Self::UnreachableEmailAddress(uri) => write!(f, "Unreachable mail address: {}", uri),
            Self::InvalidHeader(e) => e.fmt(f),
            Self::InvalidGlobPattern(e) => e.fmt(f),
            Self::MissingGitHubToken => f.write_str(
                "GitHub token not specified. To check GitHub links reliably, \
                 use `--github-token` flag / `GITHUB_TOKEN` env var.",
            ),
            Self::InvalidBase(base, e) => write!(f, "Error while base dir `{}` : {}", base, e),
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

impl From<reqwest::Error> for ErrorKind {
    fn from(e: reqwest::Error) -> Self {
        Self::ReqwestError(e)
    }
}

impl From<hubcaps::errors::Error> for ErrorKind {
    fn from(e: hubcaps::Error) -> Self {
        Self::HubcapsError(e)
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

impl From<InvalidHeaderValue> for ErrorKind {
    fn from(e: InvalidHeaderValue) -> Self {
        Self::InvalidHeader(e)
    }
}

impl From<glob::PatternError> for ErrorKind {
    fn from(e: glob::PatternError) -> Self {
        Self::InvalidGlobPattern(e)
    }
}

impl From<Infallible> for ErrorKind {
    fn from(_: Infallible) -> Self {
        // tautological
        unreachable!()
    }
}
