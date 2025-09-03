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
    pub fn try_from(input: &str) -> Result<Self, String> {
        let chars: Vec<char> = input.chars().take(3).collect();

        if matches!(
            chars.as_slice(),
            [drive, ':', sep] if drive.is_ascii_uppercase() && matches!(sep, '\\' | '/')
        ) {
            Ok(WindowsPath(input.to_string()))
        } else {
            Err(format!("'{input}' is not a Windows absolute path"))
        }
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
        assert!(WindowsPath::try_from("C:\\").is_ok());
        assert!(WindowsPath::try_from("C:\\folder").is_ok());
        assert!(WindowsPath::try_from("D:\\folder\\file.txt").is_ok());
        assert!(WindowsPath::try_from("Z:/folder/file.txt").is_ok());

        // Invalid cases
        assert!(WindowsPath::try_from("C:").is_err()); // Too short
        assert!(WindowsPath::try_from("c:\\").is_err()); // Lowercase
        assert!(WindowsPath::try_from("CC:\\").is_err()); // Two letters
        assert!(WindowsPath::try_from("C-\\").is_err()); // Not colon
        assert!(WindowsPath::try_from("C:file").is_err()); // No separator
        assert!(WindowsPath::try_from("https://example.com").is_err()); // URL
        assert!(WindowsPath::try_from("./relative").is_err()); // Relative path
    }
}
