//! Windows absolute path handling
//!
//! This module provides utilities for detecting and handling Windows absolute paths
//! to prevent them from being misinterpreted as URLs.

use std::path::Path;

/// A newtype representing a Windows absolute path
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsPath(String);

impl WindowsPath {
    /// Try to parse a string as a Windows absolute path
    pub fn try_from(input: &str) -> Option<Self> {
        let chars: Vec<char> = input.chars().take(3).collect();

        matches!(
            chars.as_slice(),
            [drive, ':', sep] if drive.is_ascii_uppercase() && matches!(sep, '\\' | '/')
        )
        .then(|| WindowsPath(input.to_string()))
    }
}

impl WindowsPath {
    /// Get a reference to the path
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}
