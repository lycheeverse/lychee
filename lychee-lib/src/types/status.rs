use std::{collections::HashSet, fmt::Display};

use http::StatusCode;
use reqwest::Response;
use serde::{Serialize, Serializer};

use crate::ErrorKind;

const ICON_OK: &str = "\u{2714}"; // ✔
const ICON_REDIRECTED: &str = "\u{21c4}"; // ⇄
const ICON_EXCLUDED: &str = "\u{003f}"; // ?
const ICON_UNSUPPORTED: &str = "\u{003f}"; // ? (using same icon, but under different name for explicitness)
const ICON_ERROR: &str = "\u{2717}"; // ✗
const ICON_TIMEOUT: &str = "\u{29d6}"; // ⧖

// TODO: is there a reasonable impl for std::ops::Try
// for this once it becomes stable?
/// Response status of the request.
#[allow(variant_size_differences)]
#[derive(Debug, Hash, PartialEq, Eq)]
pub enum Status {
    /// Request was successful
    Ok(StatusCode),
    /// Failed request
    Error(Box<ErrorKind>),
    /// Request timed out
    Timeout(Option<StatusCode>),
    /// Got redirected to different resource
    Redirected(StatusCode),
    /// Resource was excluded from checking
    Excluded,
    /// The request type is currently not supported,
    /// for example when the URL scheme is `slack://` or `file://`
    /// See https://github.com/lycheeverse/lychee/issues/199
    Unsupported(Box<ErrorKind>),
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Ok(c) => write!(f, "OK ({})", c),
            Status::Redirected(c) => write!(f, "Redirect ({})", c),
            Status::Excluded => f.write_str("Excluded"),
            Status::Timeout(Some(c)) => write!(f, "Timeout ({})", c),
            Status::Timeout(None) => f.write_str("Timeout"),
            Status::Unsupported(e) => write!(f, "Unsupported: {}", e),
            Status::Error(e) => write!(f, "Failed: {}", e),
        }
    }
}

impl Serialize for Status {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl Status {
    #[allow(clippy::missing_panics_doc)]
    #[must_use]
    /// Create a status object from a response and the set of accepted status codes
    pub fn new(response: &Response, accepted: Option<HashSet<StatusCode>>) -> Self {
        let code = response.status();

        if let Some(true) = accepted.map(|a| a.contains(&code)) {
            Self::Ok(code)
        } else {
            match response.error_for_status_ref() {
                Ok(_) if code.is_success() => Self::Ok(code),
                Ok(_) if code.is_redirection() => Self::Redirected(code),
                Err(e) => e.into(),
                Ok(_) => unreachable!(),
            }
        }
    }

    #[inline]
    #[must_use]
    /// Returns `true` if the check was successful
    pub const fn is_success(&self) -> bool {
        matches!(self, Status::Ok(_))
    }

    #[inline]
    #[must_use]
    /// Returns `true` if the check was not successful
    pub const fn is_failure(&self) -> bool {
        matches!(self, Status::Error(_))
    }

    #[inline]
    #[must_use]
    /// Returns `true` if the check was excluded
    pub const fn is_excluded(&self) -> bool {
        matches!(self, Status::Excluded)
    }

    #[inline]
    #[must_use]
    /// Returns `true` if a check took too long to complete
    pub const fn is_timeout(&self) -> bool {
        matches!(self, Status::Timeout(_))
    }

    #[inline]
    #[must_use]
    /// Returns `true` if a URI is unsupported
    pub const fn is_unsupported(&self) -> bool {
        matches!(self, Status::Unsupported(_))
    }

    #[must_use]
    /// Return a unicode icon to visualize the status
    pub const fn icon(&self) -> &str {
        match self {
            Status::Ok(_) => ICON_OK,
            Status::Redirected(_) => ICON_REDIRECTED,
            Status::Excluded => ICON_EXCLUDED,
            Status::Error(_) => ICON_ERROR,
            Status::Timeout(_) => ICON_TIMEOUT,
            Status::Unsupported(_) => ICON_UNSUPPORTED,
        }
    }
}

impl From<ErrorKind> for Status {
    fn from(e: ErrorKind) -> Self {
        Self::Error(Box::new(e))
    }
}

impl From<reqwest::Error> for Status {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Self::Timeout(e.status())
        } else if e.is_builder() {
            Self::Unsupported(Box::new(ErrorKind::ReqwestError(e)))
        } else {
            Self::Error(Box::new(ErrorKind::ReqwestError(e)))
        }
    }
}

impl From<hubcaps::Error> for Status {
    fn from(e: hubcaps::Error) -> Self {
        Self::Error(Box::new(e.into()))
    }
}
