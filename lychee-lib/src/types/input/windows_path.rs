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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_absolute_path_detection() {
        // Valid Windows absolute paths
        assert!(WindowsPath::try_from("C:\\").is_some());
        assert!(WindowsPath::try_from("C:\\folder").is_some());
        assert!(WindowsPath::try_from("D:\\folder\\file.txt").is_some());
        assert!(WindowsPath::try_from("Z:/folder/file.txt").is_some());
        
        // Invalid cases
        assert!(WindowsPath::try_from("C:").is_none()); // Too short
        assert!(WindowsPath::try_from("c:\\").is_none()); // Lowercase
        assert!(WindowsPath::try_from("CC:\\").is_none()); // Two letters
        assert!(WindowsPath::try_from("C-\\").is_none()); // Not colon
        assert!(WindowsPath::try_from("C:file").is_none()); // No separator
        assert!(WindowsPath::try_from("https://example.com").is_none()); // URL
        assert!(WindowsPath::try_from("./relative").is_none()); // Relative path
    }
}
