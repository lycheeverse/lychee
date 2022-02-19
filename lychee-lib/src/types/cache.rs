use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::Status;

/// Representation of the status of a cached request. This is kept simple on
/// purpose because the type gets serialized to a cache file and might need to
/// be parsed by other tools or edited by humans.
#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum CacheStatus {
    /// The cached request delivered a valid response
    Success,
    /// The cached request failed before
    Fail,
    /// The request was excluded (skipped)
    Excluded,
    /// The protocol is not yet supported
    Unsupported,
}

impl Display for CacheStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "Success [cached]"),
            Self::Fail => write!(f, "Fail [cached]"),
            Self::Excluded => write!(f, "Excluded [cached]"),
            Self::Unsupported => write!(f, "Unsupported [cached]"),
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
            Status::Ok(_) | Status::UnknownStatusCode(_) => Self::Success,
            Status::Excluded => Self::Excluded,
            Status::Unsupported(_) => Self::Unsupported,
            Status::Redirected(_) | Status::Error(_) | Status::Timeout(_) => Self::Fail,
        }
    }
}
