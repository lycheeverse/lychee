use serde::Serialize;
use std::fmt::Display;
use url::Url;

/// A suggestion on how to replace a broken link with a link hosted by a web archive service.
#[derive(Debug, Serialize, Eq, Hash, PartialEq)]
pub(crate) struct Suggestion {
    /// The original `Url` that was identified to be broken
    pub(crate) original: Url,
    /// The suggested `Url` replacement, which should remadiate the broken link with the use of a digital archive service.
    pub(crate) suggestion: Url,
}

impl Display for Suggestion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} --> {}", self.original, self.suggestion)?;
        Ok(())
    }
}
