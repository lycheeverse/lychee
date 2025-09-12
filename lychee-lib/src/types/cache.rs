use std::fmt::Display;

use serde::{Deserialize, Deserializer, Serialize};

use crate::{ErrorKind, Status, StatusCodeExcluder};

/// Representation of the status of a cached request. This is kept simple on
/// purpose because the type gets serialized to a cache file and might need to
/// be parsed by other tools or edited by humans.
#[derive(Debug, Serialize, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum CacheStatus {
    /// The cached request delivered a valid response
    Ok(u16),
    /// The cached request failed before
    Error(Option<u16>),
    /// The request was excluded (skipped)
    Excluded,
    /// The protocol is not yet supported
    // We no longer cache unsupported files as they might be supported in future
    // versions.
    // Nevertheless, keep for compatibility when deserializing older cache
    // files, even though this no longer gets serialized. Can be removed at a
    // later point in time.
    Unsupported,
}

impl<'de> Deserialize<'de> for CacheStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let status = <&str as Deserialize<'de>>::deserialize(deserializer)?;
        match status {
            "Excluded" => Ok(CacheStatus::Excluded),
            // Keep for compatibility with older cache files, even though this
            // no longer gets serialized. Can be removed at a later point in
            // time.
            "Unsupported" => Ok(CacheStatus::Unsupported),
            other => match other.parse::<u16>() {
                Ok(code) => match code {
                    // classify successful status codes as cache status success
                    // Does not account for status code overrides passed through
                    // the 'accept' flag. Instead, this is handled at a higher level
                    // when the cache status is converted to a status.
                    200..=299 => Ok(CacheStatus::Ok(code)),
                    // classify redirects, client errors, & server errors as cache status error
                    _ => Ok(CacheStatus::Error(Some(code))),
                },
                Err(_) => Ok(CacheStatus::Error(None)),
            },
        }
    }
}

impl Display for CacheStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ok(_) => write!(f, "OK (cached)"),
            Self::Error(_) => write!(f, "Error (cached)"),
            Self::Excluded => write!(f, "Excluded (cached)"),
            Self::Unsupported => write!(f, "Unsupported (cached)"),
        }
    }
}

impl From<&Status> for CacheStatus {
    fn from(s: &Status) -> Self {
        match s {
            Status::Cached(s) => *s,
            // Reqwest treats unknown status codes as Ok(StatusCode).
            // TODO: Use accepted status codes to decide whether this is a
            // success or failure
            Status::Ok(code) | Status::UnknownStatusCode(code) => Self::Ok(code.as_u16()),
            Status::Excluded => Self::Excluded,
            Status::Unsupported(_) => Self::Unsupported,
            Status::Redirected(code, _) => Self::Error(Some(code.as_u16())),
            Status::Timeout(code) => Self::Error(code.map(|code| code.as_u16())),
            Status::Error(e) => match e {
                ErrorKind::RejectedStatusCode(code) => Self::Error(Some(code.as_u16())),
                ErrorKind::ReadResponseBody(e) | ErrorKind::BuildRequestClient(e) => {
                    match e.status() {
                        Some(code) => Self::Error(Some(code.as_u16())),
                        None => Self::Error(None),
                    }
                }
                _ => Self::Error(None),
            },
        }
    }
}

impl From<CacheStatus> for Option<u16> {
    fn from(val: CacheStatus) -> Self {
        match val {
            CacheStatus::Ok(status) => Some(status),
            CacheStatus::Error(status) => status,
            _ => None,
        }
    }
}

impl CacheStatus {
    /// Returns `true` if the cache status is excluded by the given [`StatusCodeExcluder`].
    #[must_use]
    pub fn is_excluded(&self, excluder: &StatusCodeExcluder) -> bool {
        match Option::<u16>::from(*self) {
            Some(status) => excluder.contains(status),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde::de::value::{BorrowedStrDeserializer, Error as DeserializerError};

    use crate::CacheStatus;

    fn deserialize_cache_status(s: &str) -> Result<CacheStatus, DeserializerError> {
        let deserializer: BorrowedStrDeserializer<DeserializerError> =
            BorrowedStrDeserializer::new(s);
        CacheStatus::deserialize(deserializer)
    }

    #[test]
    fn test_deserialize_cache_status_success_code() {
        assert_eq!(deserialize_cache_status("200"), Ok(CacheStatus::Ok(200)));
    }

    #[test]
    fn test_deserialize_cache_status_error_code() {
        assert_eq!(
            deserialize_cache_status("404"),
            Ok(CacheStatus::Error(Some(404)))
        );
    }

    #[test]
    fn test_deserialize_cache_status_excluded() {
        assert_eq!(
            deserialize_cache_status("Excluded"),
            Ok(CacheStatus::Excluded)
        );
    }

    #[test]
    fn test_deserialize_cache_status_unsupported() {
        assert_eq!(
            deserialize_cache_status("Unsupported"),
            Ok(CacheStatus::Unsupported)
        );
    }

    #[test]
    fn test_deserialize_cache_status_blank() {
        assert_eq!(deserialize_cache_status(""), Ok(CacheStatus::Error(None)));
    }
}
