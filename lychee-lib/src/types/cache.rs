use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::{ErrorKind, Status};

/// Representation of the status of a cached request. This is kept simple on
/// purpose because the type gets serialized to a cache file and might need to
/// be parsed by other tools or edited by humans.
#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
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
