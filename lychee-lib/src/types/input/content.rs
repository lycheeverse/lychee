//! Input content representation and construction.
//!
//! The `InputContent` type represents the actual content extracted from various
//! input sources, along with metadata about the source and file type.

use super::source::ResolvedInputSource;
use crate::types::FileType;
use std::borrow::Cow;

/// Encapsulates the content for a given input
#[derive(Debug)]
pub struct InputContent {
    /// Input source
    pub source: ResolvedInputSource,
    /// File type of given input
    pub file_type: FileType,
    /// Raw UTF-8 string content
    pub content: String,
}

impl InputContent {
    #[must_use]
    /// Create an instance of `InputContent` from an input string
    pub fn from_string(s: &str, file_type: FileType) -> Self {
        Self {
            source: ResolvedInputSource::String(Cow::Owned(s.to_owned())),
            file_type,
            content: s.to_owned(),
        }
    }

    /// Create an instance of `InputContent` from an input string
    #[must_use]
    pub fn from_str<S: Into<Cow<'static, str>>>(s: S, file_type: FileType) -> Self {
        let cow = s.into();
        Self {
            source: ResolvedInputSource::String(cow.clone()),
            file_type,
            content: cow.into_owned(),
        }
    }
}
