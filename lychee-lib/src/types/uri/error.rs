use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use thiserror::Error;

use crate::Uri;

/// Errors that occur while parsing, validating, or resolving a URI.
///
/// These all originate from the URI/URL layer (`types/uri`, `types/base_info`,
/// input source resolution) rather than from performing a check.
#[derive(Error, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UriError {
    /// The input is empty and not accepted as a valid URL.
    #[error("Empty URL found but a URL must not be empty")]
    Empty,

    /// The given string cannot be parsed into a valid URL.
    #[error("Cannot parse '{1}' into a URL: {0}")]
    Parse(#[source] url::ParseError, String),

    /// The given string is a root-relative link and cannot be resolved without
    /// a known root directory.
    #[error("Cannot resolve root-relative link '{0}'")]
    RootRelativeWithoutRoot(String),

    /// The given path cannot be converted to a URL.
    #[error("Cannot convert path to URL: '{0}'")]
    FromPath(PathBuf),

    /// The URL is missing a host component.
    #[error("URL is missing a hostname")]
    MissingHost,

    /// The given URI is syntactically invalid.
    #[error("The given URI is invalid, check URI syntax: {0}")]
    Invalid(Uri),

    /// The given URI scheme is not supported.
    #[error("Unsupported URI type: '{0}'")]
    UnsupportedType(String),

    /// The given string cannot be parsed into a valid base URL or directory.
    #[error("Invalid base URL or directory: '{0}'. {1}")]
    InvalidBase(String, String),
}

impl UriError {
    /// Return more details about this error, including remediation hints.
    #[must_use]
    pub fn details(&self) -> String {
        match self {
            UriError::Parse(e, _url) => {
                let detail = match e {
                    url::ParseError::RelativeUrlWithoutBase => {
                        ": This relative link was found inside an input source that has no base location"
                    }
                    _ => "",
                };

                format!("{self}{detail}")
            }
            UriError::FromPath(_) => format!("{self}. Check path format"),
            UriError::UnsupportedType(_) => {
                format!("{self}. Only http, https, file, and mailto are supported")
            }
            _ => self.to_string(),
        }
    }
}

#[allow(clippy::match_same_arms)]
impl Hash for UriError {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Empty => "Empty URL".hash(state),
            Self::Parse(e, s) => (e.to_string(), s).hash(state),
            Self::RootRelativeWithoutRoot(s) => s.hash(state),
            Self::FromPath(p) => p.hash(state),
            Self::MissingHost => std::mem::discriminant(self).hash(state),
            Self::Invalid(u) => u.hash(state),
            Self::UnsupportedType(s) => s.hash(state),
            Self::InvalidBase(b, e) => (b, e).hash(state),
        }
    }
}
