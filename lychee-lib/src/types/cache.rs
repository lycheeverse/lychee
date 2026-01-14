use std::fmt::Display;

use http::StatusCode;
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

use crate::{ErrorKind, Status, StatusCodeSelector};

/// Representation of the status of a cached request. This is kept simple on
/// purpose because the type gets serialized to a cache file and might need to
/// be parsed by other tools or edited by humans.
#[derive(Debug, Serialize, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum CacheStatus {
    /// The cached request delivered a valid response
    #[serde(serialize_with = "serialize_status_code")]
    Ok(StatusCode),
    /// The cached request failed before
    #[serde(serialize_with = "serialize_optional_status_code")]
    Error(Option<StatusCode>),
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

#[allow(clippy::trivially_copy_pass_by_ref)]
fn serialize_status_code<S>(status: &StatusCode, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut s = serializer.serialize_struct("StatusCode", 1)?;
    s.serialize_field("code", &status.as_u16())?;
    s.end()
}

#[allow(clippy::trivially_copy_pass_by_ref, clippy::ref_option)]
fn serialize_optional_status_code<S>(
    status: &Option<StatusCode>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match status {
        Some(code) => serialize_status_code(code, serializer),
        None => serializer.serialize_none(),
    }
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
                Ok(code) => {
                    let code = StatusCode::from_u16(code).map_err(|_| {
                        use serde::de::Error;
                        D::Error::custom(
                            "invalid status code value, expected the value to be >= 100 and <= 999",
                        )
                    })?;
                    if code.is_success() {
                        // classify successful status codes as cache status success
                        // Does not account for status code overrides passed through
                        // the 'accept' flag. Instead, this is handled at a higher level
                        // when the cache status is converted to a status.
                        Ok(CacheStatus::Ok(code))
                    } else {
                        // classify redirects, client errors, & server errors as cache status error
                        Ok(CacheStatus::Error(Some(code)))
                    }
                }
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
            Status::Ok(code) | Status::UnknownStatusCode(code) => Self::Ok(*code),
            Status::Excluded => Self::Excluded,
            Status::Unsupported(_) => Self::Unsupported,
            Status::Redirected(code, _) => Self::Error(Some(*code)),
            Status::Timeout(code) => Self::Error(*code),
            Status::Error(e) => match e {
                ErrorKind::RejectedStatusCode(code) => Self::Error(Some(*code)),
                ErrorKind::ReadResponseBody(e) | ErrorKind::BuildRequestClient(e) => {
                    match e.status() {
                        Some(code) => Self::Error(Some(code)),
                        None => Self::Error(None),
                    }
                }
                _ => Self::Error(None),
            },
            Status::RequestError(_) => Self::Error(None),
        }
    }
}

impl From<CacheStatus> for Option<StatusCode> {
    fn from(val: CacheStatus) -> Self {
        match val {
            CacheStatus::Ok(status) => Some(status),
            CacheStatus::Error(status) => status,
            _ => None,
        }
    }
}

impl CacheStatus {
    /// Returns `true` if the cache status is excluded by the given [`StatusCodeSelector`].
    #[must_use]
    pub fn is_excluded(&self, excluder: &StatusCodeSelector) -> bool {
        match Option::<StatusCode>::from(*self) {
            Some(status) => excluder.contains(status.as_u16()),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use http::StatusCode;
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
        assert_eq!(
            deserialize_cache_status("200"),
            Ok(CacheStatus::Ok(StatusCode::OK))
        );
    }

    #[test]
    fn test_deserialize_cache_status_error_code() {
        assert_eq!(
            deserialize_cache_status("404"),
            Ok(CacheStatus::Error(Some(StatusCode::NOT_FOUND)))
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
