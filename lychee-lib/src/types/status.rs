use std::{collections::HashSet, fmt::Display};

use super::CacheStatus;
use super::redirect_history::Redirects;
use crate::ErrorKind;
use http::StatusCode;
use reqwest::Response;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

const ICON_OK: &str = "✔";
const ICON_REDIRECTED: &str = "⇄";
const ICON_EXCLUDED: &str = "?";
const ICON_UNSUPPORTED: &str = "\u{003f}"; // ? (using same icon, but under different name for explicitness)
const ICON_UNKNOWN: &str = "?";
const ICON_ERROR: &str = "✗";
const ICON_TIMEOUT: &str = "⧖";
const ICON_CACHED: &str = "↻";

/// Response status of the request.
#[allow(variant_size_differences)]
#[derive(Debug, Hash, PartialEq, Eq)]
pub enum Status {
    /// Request was successful
    Ok(StatusCode),
    /// Failed request
    Error(ErrorKind),
    /// Request timed out
    Timeout(Option<StatusCode>),
    /// Got redirected to different resource
    Redirected(StatusCode, Redirects),
    /// The given status code is not known by lychee
    UnknownStatusCode(StatusCode),
    /// Resource was excluded from checking
    Excluded,
    /// The request type is currently not supported,
    /// for example when the URL scheme is `slack://`.
    /// See <https://github.com/lycheeverse/lychee/issues/199>
    Unsupported(ErrorKind),
    /// Cached request status from previous run
    Cached(CacheStatus),
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Ok(code) => write!(f, "{code}"),
            Status::Redirected(_, _) => write!(f, "Redirect"),
            Status::UnknownStatusCode(code) => write!(f, "Unknown status ({code})"),
            Status::Timeout(Some(code)) => write!(f, "Timeout ({code})"),
            Status::Timeout(None) => f.write_str("Timeout"),
            Status::Unsupported(e) => write!(f, "Unsupported: {e}"),
            Status::Error(e) => write!(f, "{e}"),
            Status::Cached(status) => write!(f, "{status}"),
            Status::Excluded => Ok(()),
        }
    }
}

impl Serialize for Status {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s;

        if let Some(code) = self.code() {
            s = serializer.serialize_struct("Status", 2)?;
            s.serialize_field("text", &self.to_string())?;
            s.serialize_field("code", &code.as_u16())?;
        } else if let Some(details) = self.details() {
            s = serializer.serialize_struct("Status", 2)?;
            s.serialize_field("text", &self.to_string())?;
            s.serialize_field("details", &details.to_string())?;
        } else {
            s = serializer.serialize_struct("Status", 1)?;
            s.serialize_field("text", &self.to_string())?;
        }

        if let Status::Redirected(_, redirects) = self {
            s.serialize_field("redirects", redirects)?;
        }

        s.end()
    }
}

impl Status {
    #[must_use]
    /// Create a status object from a response and the set of accepted status codes
    pub fn new(response: &Response, accepted: &HashSet<StatusCode>) -> Self {
        let code = response.status();

        if accepted.contains(&code) {
            Self::Ok(code)
        } else {
            Self::Error(ErrorKind::RejectedStatusCode(code))
        }
    }

    /// Create a status object from a cached status (from a previous run of
    /// lychee) and the set of accepted status codes.
    ///
    /// The set of accepted status codes can change between runs,
    /// necessitating more complex logic than just using the cached status.
    ///
    /// Note that the accepted status codes are not of type `StatusCode`,
    /// because they are provided by the user and can be invalid according to
    /// the HTTP spec and IANA, but the user might still want to accept them.
    #[must_use]
    pub fn from_cache_status(s: CacheStatus, accepted: &HashSet<u16>) -> Self {
        match s {
            CacheStatus::Ok(code) => {
                if matches!(s, CacheStatus::Ok(_)) || accepted.contains(&code) {
                    return Self::Cached(CacheStatus::Ok(code));
                }
                Self::Cached(CacheStatus::Error(Some(code)))
            }
            CacheStatus::Error(code) => {
                if let Some(code) = code {
                    if accepted.contains(&code) {
                        return Self::Cached(CacheStatus::Ok(code));
                    }
                }
                Self::Cached(CacheStatus::Error(code))
            }
            _ => Self::Cached(s),
        }
    }

    /// Return more details about the status (if any)
    ///
    /// Which additional information we can extract depends on the underlying
    /// request type. The output is purely meant for humans and future changes
    /// are expected.
    ///
    /// It is modeled after reqwest's `details` method.
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub fn details(&self) -> Option<String> {
        match &self {
            Status::Ok(code) => code.canonical_reason().map(String::from),
            Status::Redirected(code, redirects) => {
                let count = redirects.count();
                let noun = if count == 1 { "redirect" } else { "redirects" };

                let result = code
                    .canonical_reason()
                    .map(String::from)
                    .unwrap_or(code.as_str().to_owned());
                Some(format!(
                    "Followed {count} {noun} resolving to the final status of: {result}. Redirects: {redirects}"
                ))
            }
            Status::Error(e) => e.details(),
            Status::Timeout(_) => None,
            Status::UnknownStatusCode(_) => None,
            Status::Unsupported(_) => None,
            Status::Cached(_) => None,
            Status::Excluded => None,
        }
    }

    #[inline]
    #[must_use]
    /// Returns `true` if the check was successful
    pub const fn is_success(&self) -> bool {
        matches!(self, Status::Ok(_) | Status::Cached(CacheStatus::Ok(_)))
    }

    #[inline]
    #[must_use]
    /// Returns `true` if the check was not successful
    pub const fn is_error(&self) -> bool {
        matches!(
            self,
            Status::Error(_) | Status::Cached(CacheStatus::Error(_)) | Status::Timeout(_)
        )
    }

    #[inline]
    #[must_use]
    /// Returns `true` if the check was excluded
    pub const fn is_excluded(&self) -> bool {
        matches!(
            self,
            Status::Excluded | Status::Cached(CacheStatus::Excluded)
        )
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
        matches!(
            self,
            Status::Unsupported(_) | Status::Cached(CacheStatus::Unsupported)
        )
    }

    #[must_use]
    /// Return a unicode icon to visualize the status
    pub const fn icon(&self) -> &str {
        match self {
            Status::Ok(_) => ICON_OK,
            Status::Redirected(_, _) => ICON_REDIRECTED,
            Status::UnknownStatusCode(_) => ICON_UNKNOWN,
            Status::Excluded => ICON_EXCLUDED,
            Status::Error(_) => ICON_ERROR,
            Status::Timeout(_) => ICON_TIMEOUT,
            Status::Unsupported(_) => ICON_UNSUPPORTED,
            Status::Cached(_) => ICON_CACHED,
        }
    }

    #[must_use]
    /// Return the HTTP status code (if any)
    pub fn code(&self) -> Option<StatusCode> {
        match self {
            Status::Ok(code)
            | Status::Redirected(code, _)
            | Status::UnknownStatusCode(code)
            | Status::Timeout(Some(code)) => Some(*code),
            Status::Error(kind) | Status::Unsupported(kind) => match kind {
                ErrorKind::RejectedStatusCode(status_code) => Some(*status_code),
                _ => match kind.reqwest_error() {
                    Some(error) => error.status(),
                    None => None,
                },
            },
            Status::Cached(CacheStatus::Ok(code) | CacheStatus::Error(Some(code))) => {
                StatusCode::from_u16(*code).ok()
            }
            _ => None,
        }
    }

    /// Return the HTTP status code as string (if any)
    #[must_use]
    pub fn code_as_string(&self) -> String {
        match self {
            Status::Ok(code) | Status::Redirected(code, _) | Status::UnknownStatusCode(code) => {
                code.as_str().to_string()
            }
            Status::Excluded => "EXCLUDED".to_string(),
            Status::Error(e) => match e {
                ErrorKind::RejectedStatusCode(code) => code.as_str().to_string(),
                ErrorKind::ReadResponseBody(e) | ErrorKind::BuildRequestClient(e) => {
                    match e.status() {
                        Some(code) => code.as_str().to_string(),
                        None => "ERROR".to_string(),
                    }
                }
                _ => "ERROR".to_string(),
            },
            Status::Timeout(code) => match code {
                Some(code) => code.as_str().to_string(),
                None => "TIMEOUT".to_string(),
            },
            Status::Unsupported(_) => "IGNORED".to_string(),
            Status::Cached(cache_status) => match cache_status {
                CacheStatus::Ok(code) => code.to_string(),
                CacheStatus::Error(code) => match code {
                    Some(code) => code.to_string(),
                    None => "ERROR".to_string(),
                },
                CacheStatus::Excluded => "EXCLUDED".to_string(),
                CacheStatus::Unsupported => "IGNORED".to_string(),
            },
        }
    }

    /// Returns true if the status code is unknown
    /// (i.e. not a valid HTTP status code)
    ///
    /// For example, `200` is a valid HTTP status code,
    /// while `999` is not.
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(self, Status::UnknownStatusCode(_))
    }
}

impl From<ErrorKind> for Status {
    fn from(e: ErrorKind) -> Self {
        Self::Error(e)
    }
}

impl From<reqwest::Error> for Status {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Self::Timeout(e.status())
        } else if e.is_builder() {
            Self::Unsupported(ErrorKind::BuildRequestClient(e))
        } else if e.is_body() || e.is_decode() {
            Self::Unsupported(ErrorKind::ReadResponseBody(e))
        } else {
            Self::Error(ErrorKind::NetworkRequest(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{CacheStatus, ErrorKind, Status, types::redirect_history::Redirects};
    use http::StatusCode;

    #[test]
    fn test_status_serialization() {
        let status_ok = Status::Ok(StatusCode::from_u16(200).unwrap());
        let serialized_with_code = serde_json::to_string(&status_ok).unwrap();
        assert_eq!("{\"text\":\"200 OK\",\"code\":200}", serialized_with_code);

        let status_timeout = Status::Timeout(None);
        let serialized_without_code = serde_json::to_string(&status_timeout).unwrap();
        assert_eq!("{\"text\":\"Timeout\"}", serialized_without_code);
    }

    #[test]
    fn test_get_status_code() {
        assert_eq!(
            Status::Ok(StatusCode::from_u16(200).unwrap())
                .code()
                .unwrap(),
            200
        );
        assert_eq!(
            Status::Timeout(Some(StatusCode::from_u16(408).unwrap()))
                .code()
                .unwrap(),
            408
        );
        assert_eq!(
            Status::UnknownStatusCode(StatusCode::from_u16(999).unwrap())
                .code()
                .unwrap(),
            999
        );
        assert_eq!(
            Status::Redirected(StatusCode::from_u16(300).unwrap(), Redirects::none())
                .code()
                .unwrap(),
            300
        );
        assert_eq!(Status::Cached(CacheStatus::Ok(200)).code().unwrap(), 200);
        assert_eq!(
            Status::Cached(CacheStatus::Error(Some(404)))
                .code()
                .unwrap(),
            404
        );
        assert_eq!(Status::Timeout(None).code(), None);
        assert_eq!(Status::Cached(CacheStatus::Error(None)).code(), None);
        assert_eq!(Status::Excluded.code(), None);
        assert_eq!(
            Status::Unsupported(ErrorKind::InvalidStatusCode(999)).code(),
            None
        );
    }

    #[test]
    fn test_status_unknown() {
        assert!(Status::UnknownStatusCode(StatusCode::from_u16(999).unwrap()).is_unknown());
        assert!(!Status::Ok(StatusCode::from_u16(200).unwrap()).is_unknown());
    }
}
