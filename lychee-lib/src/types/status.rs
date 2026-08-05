use std::{collections::HashSet, fmt::Display};

use super::CacheStatus;
use crate::ErrorKind;
use crate::RequestError;
use crate::ratelimit::CacheableResponse;
use http::StatusCode;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

const ICON_OK: &str = "✔";
const ICON_EXCLUDED: &str = "?";
const ICON_UNSUPPORTED: &str = "\u{003f}"; // ? (using same icon, but under different name for explicitness)
const ICON_UNKNOWN: &str = "?";
const ICON_ERROR: &str = "✗";
const ICON_TIMEOUT: &str = "⧖";
const ICON_CACHED: &str = "↻";

/// The reason why a resource was excluded from checking.
#[derive(Debug, Hash, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExcludeReason {
    /// A user-provided exclude pattern matched the resource.
    Pattern(String),
    /// Mail checking is disabled.
    Mail,
    /// The URI scheme is not among the accepted schemes.
    Scheme(String),
    /// The host is excluded.
    Host(String),
    /// The IP address is in the loopback range.
    LoopbackIp(String),
    /// The IP address is in a private range.
    PrivateIp(String),
    /// The IP address is in the link-local range.
    LinkLocalIp(String),
    /// Telephone links are not checked.
    Telephone,
    /// Reserved example domains are not checked.
    ExampleDomain,
    /// The domain is not supported.
    UnsupportedDomain,
    /// The resource is a known false positive.
    FalsePositive,
    /// No user-provided include pattern matched the resource.
    NotIncluded,
    /// Mail checking support is not enabled in this build.
    MailFeatureDisabled,
    /// A request chain excluded the resource.
    RequestChain,
}

impl Display for ExcludeReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pattern(pattern) => write!(f, "Excluded by pattern: `{pattern}`"),
            Self::Mail => {
                f.write_str("Excluded: mail checking is disabled (use --include-mail to enable)")
            }
            Self::Scheme(scheme) => {
                write!(
                    f,
                    "Excluded: scheme `{scheme}` is not among the accepted schemes"
                )
            }
            Self::Host(host) => write!(
                f,
                "Excluded: host `{host}` resolves to a loopback address (use --exclude-loopback=false to check it)"
            ),
            Self::LoopbackIp(addr) => write!(
                f,
                "Excluded: `{addr}` is a loopback IP address (use --exclude-loopback=false to check it)"
            ),
            Self::PrivateIp(addr) => write!(
                f,
                "Excluded: `{addr}` is a private IP address (use --exclude-private=false to check it)"
            ),
            Self::LinkLocalIp(addr) => write!(
                f,
                "Excluded: `{addr}` is a link-local IP address (use --exclude-link-local=false to check it)"
            ),
            Self::Telephone => f.write_str("Excluded: telephone links are not checked"),
            Self::ExampleDomain => f.write_str("Excluded: example domains are not checked"),
            Self::UnsupportedDomain => f.write_str("Excluded: domain is not supported"),
            Self::FalsePositive => f.write_str("Excluded: known false positive"),
            Self::NotIncluded => f.write_str("Excluded: no include pattern matched"),
            Self::MailFeatureDisabled => {
                f.write_str("Excluded: mail checking support is not enabled in this build")
            }
            Self::RequestChain => f.write_str("Excluded by request chain"),
        }
    }
}

/// Response status of the request.
#[allow(variant_size_differences)]
#[derive(Debug, Hash, PartialEq, Eq)]
pub enum Status {
    /// Request was successful
    Ok(StatusCode),
    /// Failed request
    Error(ErrorKind),
    /// Request could not be built
    RequestError(RequestError),
    /// Request timed out
    Timeout(Option<StatusCode>),
    /// The given status code is not known by lychee
    UnknownStatusCode(StatusCode),
    /// The given mail address could not be reliably identified.
    /// This normally happens due to restrictive measures by
    /// mail servers (blocklisting) or your ISP (port filtering).
    UnknownMailStatus(String),
    /// Resource was excluded from checking
    Excluded(ExcludeReason),
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
            Status::UnknownStatusCode(code) => write!(f, "Unknown status ({code})"),
            Status::UnknownMailStatus(_) => write!(f, "Unknown mail status"),
            Status::Timeout(Some(code)) => write!(f, "Timeout ({code})"),
            Status::Timeout(None) => f.write_str("Timeout"),
            Status::Unsupported(e) => write!(f, "Unsupported: {e}"),
            Status::Error(e) => write!(f, "{e}"),
            Status::RequestError(e) => write!(f, "{e}"),
            Status::Cached(status) => write!(f, "{status}"),
            Status::Excluded(_) => f.write_str("Excluded"),
        }
    }
}

impl Serialize for Status {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s;

        if let Some(code) = self.code() {
            s = serializer.serialize_struct("Status", 2)?;
            let mut s = s;
            s.serialize_field("text", &self.to_string())?;
            s.serialize_field("code", &code.as_u16())?;
            s.end()
        } else {
            s = serializer.serialize_struct("Status", 2)?;
            let mut s = s;
            s.serialize_field("text", &self.to_string())?;
            s.serialize_field("details", &self.details())?;
            s.end()
        }
    }
}

impl Status {
    /// Create a status object from a response and the set of accepted status codes
    #[must_use]
    pub(crate) fn new(response: &CacheableResponse, accepted: &HashSet<StatusCode>) -> Self {
        let status = response.status;
        if accepted.contains(&status) {
            Self::Ok(status)
        } else {
            Self::Error(ErrorKind::RejectedStatusCode(status))
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
    pub fn from_cache_status(s: CacheStatus, accepted: &HashSet<StatusCode>) -> Self {
        match s {
            CacheStatus::Ok(code) => {
                if matches!(s, CacheStatus::Ok(_)) || accepted.contains(&code) {
                    return Self::Cached(CacheStatus::Ok(code));
                }
                Self::Cached(CacheStatus::Error(Some(code)))
            }
            CacheStatus::Error(code) => {
                if let Some(code) = code
                    && accepted.contains(&code)
                {
                    return Self::Cached(CacheStatus::Ok(code));
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
    pub fn details(&self) -> String {
        match &self {
            Status::Ok(code) => code.to_string(),
            Status::Error(e) => e.details(),
            Status::RequestError(e) => e.error().details(),
            Status::UnknownMailStatus(reason) => reason.clone(),
            Status::Timeout(_) => "Request timed out".into(),
            Status::Excluded(reason) => reason.to_string(),
            Status::Unsupported(_) | Status::Cached(_) | Status::UnknownStatusCode(_) => {
                self.to_string()
            }
        }
    }

    /// Returns `true` if the check was successful
    #[inline]
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, Status::Ok(_) | Status::Cached(CacheStatus::Ok(_)))
    }

    /// Returns `true` if the check was not successful
    #[inline]
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(
            self,
            Status::Error(_)
                | Status::RequestError(_)
                | Status::Cached(CacheStatus::Error(_))
                | Status::Timeout(_)
        )
    }

    /// Returns `true` if the check was excluded
    #[inline]
    #[must_use]
    pub const fn is_excluded(&self) -> bool {
        matches!(
            self,
            Status::Excluded(_) | Status::Cached(CacheStatus::Excluded)
        )
    }

    /// Returns `true` if a check took too long to complete
    #[inline]
    #[must_use]
    pub const fn is_timeout(&self) -> bool {
        matches!(self, Status::Timeout(_))
    }

    /// Returns `true` if a URI is unsupported
    #[inline]
    #[must_use]
    pub const fn is_unsupported(&self) -> bool {
        matches!(
            self,
            Status::Unsupported(_) | Status::Cached(CacheStatus::Unsupported)
        )
    }

    /// Returns true if the status code is unknown
    /// (i.e. not a valid HTTP status code)
    ///
    /// For example, `200` is a valid HTTP status code,
    /// while `999` is not.
    #[inline]
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(self, Status::UnknownStatusCode(_))
    }

    /// Return a unicode icon to visualize the status
    #[must_use]
    pub const fn icon(&self) -> &str {
        match self {
            Status::Ok(_) => ICON_OK,
            Status::UnknownStatusCode(_) | Status::UnknownMailStatus(_) => ICON_UNKNOWN,
            Status::Excluded(_) => ICON_EXCLUDED,
            Status::Error(_) | Status::RequestError(_) => ICON_ERROR,
            Status::Timeout(_) => ICON_TIMEOUT,
            Status::Unsupported(_) => ICON_UNSUPPORTED,
            Status::Cached(_) => ICON_CACHED,
        }
    }

    /// Return the HTTP status code (if any)
    #[must_use]
    pub fn code(&self) -> Option<StatusCode> {
        match self {
            Status::Ok(code)
            | Status::UnknownStatusCode(code)
            | Status::Timeout(Some(code))
            | Status::Cached(CacheStatus::Ok(code) | CacheStatus::Error(Some(code))) => Some(*code),
            Status::Error(kind) | Status::Unsupported(kind) => match kind {
                ErrorKind::RejectedStatusCode(status_code) => Some(*status_code),
                _ => match kind.reqwest_error() {
                    Some(error) => error.status(),
                    None => None,
                },
            },
            _ => None,
        }
    }

    /// Return the HTTP status code as string (if any)
    #[must_use]
    pub fn code_as_string(&self) -> String {
        match self {
            Status::Ok(code) | Status::UnknownStatusCode(code) => code.as_u16().to_string(),
            Status::UnknownMailStatus(_) => "UNKNOWN".to_string(),
            Status::Excluded(_) => "EXCLUDED".to_string(),
            Status::Error(e) => match e {
                ErrorKind::RejectedStatusCode(code) => code.as_u16().to_string(),
                ErrorKind::ReadResponseBody(e) | ErrorKind::BuildRequestClient(e) => {
                    match e.status() {
                        Some(code) => code.as_u16().to_string(),
                        None => "ERROR".to_string(),
                    }
                }
                _ => "ERROR".to_string(),
            },
            Status::RequestError(_) => "ERROR".to_string(),
            Status::Timeout(code) => match code {
                Some(code) => code.as_u16().to_string(),
                None => "TIMEOUT".to_string(),
            },
            Status::Unsupported(_) => "IGNORED".to_string(),
            Status::Cached(cache_status) => match cache_status {
                CacheStatus::Ok(code) => code.as_u16().to_string(),
                CacheStatus::Error(code) => match code {
                    Some(code) => code.as_u16().to_string(),
                    None => "ERROR".to_string(),
                },
                CacheStatus::Excluded => "EXCLUDED".to_string(),
                CacheStatus::Unsupported => "IGNORED".to_string(),
            },
        }
    }
}

impl From<ErrorKind> for Status {
    fn from(e: ErrorKind) -> Self {
        match e {
            ErrorKind::InvalidUrlHost => Status::Unsupported(ErrorKind::InvalidUrlHost),
            ErrorKind::NetworkRequest(e)
            | ErrorKind::ReadResponseBody(e)
            | ErrorKind::BuildRequestClient(e) => {
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
            e => Self::Error(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{CacheStatus, ErrorKind, ExcludeReason, Status};
    use http::StatusCode;

    /// The rendered reason is what users see for every excluded link, so pin
    /// down the wording of every variant.
    ///
    /// This also covers the variants that cannot be reached from the filter
    /// unit tests: `ExampleDomain` (the example domain set is empty under
    /// `cfg(test)`) and `MailFeatureDisabled` (only produced when the
    /// `email-check` feature is off).
    #[test]
    fn test_exclude_reason_messages() {
        let cases = [
            (
                ExcludeReason::Pattern(r"example\.com".to_owned()),
                r"Excluded by pattern: `example\.com`",
            ),
            (
                ExcludeReason::Mail,
                "Excluded: mail checking is disabled (use --include-mail to enable)",
            ),
            (
                ExcludeReason::Scheme("http".to_owned()),
                "Excluded: scheme `http` is not among the accepted schemes",
            ),
            (
                ExcludeReason::Host("localhost".to_owned()),
                "Excluded: host `localhost` resolves to a loopback address (use --exclude-loopback=false to check it)",
            ),
            (
                ExcludeReason::LoopbackIp("127.0.0.1".to_owned()),
                "Excluded: `127.0.0.1` is a loopback IP address (use --exclude-loopback=false to check it)",
            ),
            (
                ExcludeReason::PrivateIp("192.168.0.1".to_owned()),
                "Excluded: `192.168.0.1` is a private IP address (use --exclude-private=false to check it)",
            ),
            (
                ExcludeReason::LinkLocalIp("169.254.0.1".to_owned()),
                "Excluded: `169.254.0.1` is a link-local IP address (use --exclude-link-local=false to check it)",
            ),
            (
                ExcludeReason::Telephone,
                "Excluded: telephone links are not checked",
            ),
            (
                ExcludeReason::ExampleDomain,
                "Excluded: example domains are not checked",
            ),
            (
                ExcludeReason::UnsupportedDomain,
                "Excluded: domain is not supported",
            ),
            (
                ExcludeReason::FalsePositive,
                "Excluded: known false positive",
            ),
            (
                ExcludeReason::NotIncluded,
                "Excluded: no include pattern matched",
            ),
            (
                ExcludeReason::MailFeatureDisabled,
                "Excluded: mail checking support is not enabled in this build",
            ),
            (ExcludeReason::RequestChain, "Excluded by request chain"),
        ];

        for (reason, expected) in cases {
            assert_eq!(reason.to_string(), expected);
            // The reason is what ends up in the report and in JSON output
            assert_eq!(Status::Excluded(reason).details(), expected);
        }
    }

    #[test]
    fn test_status_serialization() {
        let status_ok = Status::Ok(StatusCode::from_u16(200).unwrap());
        let serialized_with_code = serde_json::to_string(&status_ok).unwrap();
        assert_eq!(r#"{"text":"200 OK","code":200}"#, serialized_with_code);

        let status_error = Status::Error(ErrorKind::EmptyUrl);
        let serialized_with_error = serde_json::to_string(&status_error).unwrap();
        assert_eq!(
            r#"{"text":"Empty URL found but a URL must not be empty","details":"Empty URL found but a URL must not be empty"}"#,
            serialized_with_error
        );

        let status_timeout = Status::Timeout(None);
        let serialized_without_code = serde_json::to_string(&status_timeout).unwrap();
        assert_eq!(
            r#"{"text":"Timeout","details":"Request timed out"}"#,
            serialized_without_code
        );
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
            Status::Cached(CacheStatus::Ok(StatusCode::OK))
                .code()
                .unwrap(),
            200
        );
        assert_eq!(
            Status::Cached(CacheStatus::Error(Some(StatusCode::NOT_FOUND)))
                .code()
                .unwrap(),
            404
        );
        assert_eq!(Status::Timeout(None).code(), None);
        assert_eq!(Status::Cached(CacheStatus::Error(None)).code(), None);
        assert_eq!(Status::Excluded(ExcludeReason::Mail).code(), None);
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
