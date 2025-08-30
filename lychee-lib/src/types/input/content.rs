//! Input content representation and construction.
//!
//! The `InputContent` type represents the actual content extracted from various
//! input sources, along with metadata about the source and file type.

use super::source::InputSource;
use super::source::ResolvedInputSource;
use crate::ErrorKind;
use crate::types::FileType;
use std::fs;
use std::path::PathBuf;

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
        // TODO: consider using Cow (to avoid one .clone() for String types)
        Self {
            source: ResolvedInputSource::String(s.to_owned()),
            file_type,
            content: s.to_owned(),
        }
    }
}

impl TryFrom<&PathBuf> for InputContent {
    type Error = crate::ErrorKind;

    fn try_from(path: &PathBuf) -> std::result::Result<Self, Self::Error> {
        let input = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                log::warn!(
                    "Skipping file with invalid UTF-8 content: {}",
                    path.display()
                );
                return Err(ErrorKind::ReadFileInput(e, path.clone()));
            }
            Err(e) => return Err(ErrorKind::ReadFileInput(e, path.clone())),
        };

        Ok(Self {
            source: ResolvedInputSource::String(input.clone()),
            file_type: FileType::from(path),
            content: input,
        })
    }
}
