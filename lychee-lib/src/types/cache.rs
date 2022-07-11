use std::fmt::Display;

use serde::{Deserialize, Deserializer, Serialize};

use crate::{ErrorKind, Status};

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
            "Unsupported" => Ok(CacheStatus::Unsupported),
            other => match other.parse::<u16>() {
                Ok(code) => match code {
                    // classify successful status codes as cache status success
                    200..=299 => Ok(CacheStatus::Ok(code)),
                    // classify redirects, client errors, & server errors as cache status errors
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
            Status::Redirected(code) => Self::Error(Some(code.as_u16())),
            Status::Timeout(code) => Self::Error(code.map(|code| code.as_u16())),
            Status::Error(e) => match e {
                ErrorKind::NetworkRequest(e)
                | ErrorKind::ReadResponseBody(e)
                | ErrorKind::BuildRequestClient(e) => match e.status() {
                    Some(code) => Self::Error(Some(code.as_u16())),
                    None => Self::Error(None),
                },
                _ => Self::Error(None),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::de::value::{BorrowedStrDeserializer, Error as DeserializerError};
    use serde::Deserialize;

    use crate::CacheStatus;

    #[test]
    fn test_deserialize_cache_status_success_code() {
        let deserializer: BorrowedStrDeserializer<DeserializerError> =
            BorrowedStrDeserializer::new("200");
        assert_eq!(
            CacheStatus::deserialize(deserializer),
            Ok(CacheStatus::Ok(200))
        );
    }

    #[test]
    fn test_deserialize_cache_status_error_code() {
        let deserializer: BorrowedStrDeserializer<DeserializerError> =
            BorrowedStrDeserializer::new("404");
        assert_eq!(
            CacheStatus::deserialize(deserializer),
            Ok(CacheStatus::Error(Some(404)))
        );
    }

    #[test]
    fn test_deserialize_cache_status_excluded() {
        let deserializer: BorrowedStrDeserializer<DeserializerError> =
            BorrowedStrDeserializer::new("Excluded");
        assert_eq!(
            CacheStatus::deserialize(deserializer),
            Ok(CacheStatus::Excluded)
        );
    }

    #[test]
    fn test_deserialize_cache_status_unsupported() {
        let deserializer: BorrowedStrDeserializer<DeserializerError> =
            BorrowedStrDeserializer::new("Unsupported");
        assert_eq!(
            CacheStatus::deserialize(deserializer),
            Ok(CacheStatus::Unsupported)
        );
    }

    #[test]
    fn test_deserialize_cache_status_blank() {
        let deserializer: BorrowedStrDeserializer<DeserializerError> =
            BorrowedStrDeserializer::new("");
        assert_eq!(
            CacheStatus::deserialize(deserializer),
            Ok(CacheStatus::Error(None))
        );
    }
}
