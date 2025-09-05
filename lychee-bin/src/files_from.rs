//! File list reading functionality for --files-from option
//!
//! This module provides the `FilesFrom` struct which handles reading input file
//! lists from any reader, with support for comments and empty line filtering.

use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

/// Comment marker for ignoring lines in files-from input
const COMMENT_MARKER: &str = "#";

/// Represents a source of input file paths that can be read from any reader
#[derive(Debug, Clone)]
pub(crate) struct FilesFrom {
    /// The list of input file paths
    pub(crate) inputs: Vec<String>,
}

impl FilesFrom {
    /// Create `FilesFrom` from any reader
    pub(crate) fn from_reader<R: Read>(reader: R) -> Result<Self> {
        let buf_reader = BufReader::new(reader);
        let lines: Vec<String> = buf_reader
            .lines()
            .collect::<Result<Vec<_>, _>>()
            .context("Cannot read lines from reader")?;

        let inputs = Self::filter_lines(lines);
        Ok(FilesFrom { inputs })
    }

    /// Filter out comments and empty lines from input
    fn filter_lines(lines: Vec<String>) -> Vec<String> {
        lines
            .into_iter()
            .filter(|line| {
                let line = line.trim();
                !line.is_empty() && !line.starts_with(COMMENT_MARKER)
            })
            .collect()
    }
}

impl TryFrom<&Path> for FilesFrom {
    type Error = anyhow::Error;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        if path == Path::new("-") {
            Self::from_reader(std::io::stdin())
        } else {
            let file = std::fs::File::open(path)
                .with_context(|| format!("Cannot open --files-from file: {}", path.display()))?;
            Self::from_reader(file)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Cursor;
    use tempfile::tempdir;

    #[test]
    fn test_filter_lines() {
        let input = vec![
            "file1.md".to_string(),
            String::new(),
            "# This is a comment".to_string(),
            "file2.md".to_string(),
            "   ".to_string(),
            "  # Another comment".to_string(),
            "file3.md".to_string(),
        ];

        let result = FilesFrom::filter_lines(input);
        assert_eq!(result, vec!["file1.md", "file2.md", "file3.md"]);
    }

    #[test]
    fn test_from_reader() -> Result<()> {
        let input = "# Comment\nfile1.md\n\nfile2.md\n# Another comment\nfile3.md\n";
        let reader = Cursor::new(input);

        let files_from = FilesFrom::from_reader(reader)?;
        assert_eq!(files_from.inputs, vec!["file1.md", "file2.md", "file3.md"]);

        Ok(())
    }

    #[test]
    fn test_from_reader_empty() -> Result<()> {
        let input = "# Only comments\n\n# More comments\n   \n";
        let reader = Cursor::new(input);

        let files_from = FilesFrom::from_reader(reader)?;
        assert_eq!(files_from.inputs, Vec::<String>::new());

        Ok(())
    }

    #[test]
    fn test_try_from_file() -> Result<()> {
        let temp_dir = tempdir()?;
        let file_path = temp_dir.path().join("files.txt");

        fs::write(
            &file_path,
            "# Comment\nfile1.md\n\nfile2.md\n# Another comment\nfile3.md\n",
        )?;

        let files_from = FilesFrom::try_from(file_path.as_path())?;
        assert_eq!(files_from.inputs, vec!["file1.md", "file2.md", "file3.md"]);

        Ok(())
    }

    #[test]
    fn test_try_from_nonexistent_file() {
        let result = FilesFrom::try_from(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Cannot open --files-from file")
        );
    }
}
