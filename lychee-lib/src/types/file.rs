use ignore::types::{Types, TypesBuilder};
use serde::{Deserialize, Serialize};
use std::path::Path;
use url::Url;

/// Represents an ordered list of file extensions.
///
/// This holds the actual extension strings (e.g. `md`, `html`, etc.) and is
/// used to build a [`Types`] object which can be used to match file types.
///
/// In a sense, it is more "low-level" than [`FileType`] as it is closer to the
/// actual representation of file extensions, while [`FileType`] is a higher-level
/// abstraction that represents the "category" of a file (e.g. Markdown, HTML).
///
/// The order is significant as extensions at the beginning of the vector will
/// be treated with higher priority (e.g. when deciding which file to pick out
/// of a set of options)
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct FileExtensions(Vec<String>);

impl Default for FileExtensions {
    fn default() -> Self {
        FileType::default_extensions()
    }
}

impl std::fmt::Display for FileExtensions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.join(","))
    }
}

impl FileExtensions {
    /// Create an empty list of file extensions
    #[must_use]
    pub const fn empty() -> Self {
        Self(vec![])
    }

    /// Extend the list of existing extensions by the values from the iterator
    pub fn extend<I: IntoIterator<Item = String>>(&mut self, iter: I) {
        self.0.extend(iter);
    }

    /// Check if the list of file extensions contains the given file extension
    pub fn contains<T: Into<String>>(&self, file_extension: T) -> bool {
        self.0.contains(&file_extension.into())
    }
}

impl TryFrom<FileExtensions> for Types {
    type Error = super::ErrorKind;

    /// Build the current list of file extensions into a file type matcher.
    ///
    /// # Errors
    ///
    /// Fails if an extension is `all` or otherwise contains any character that
    /// is not a Unicode letter or number.
    fn try_from(value: FileExtensions) -> super::Result<Self> {
        let mut types_builder = TypesBuilder::new();
        for ext in value.0.clone() {
            types_builder.add(&ext, &format!("*.{ext}"))?;
        }
        Ok(types_builder.select("all").build()?)
    }
}

impl From<FileExtensions> for Vec<String> {
    fn from(value: FileExtensions) -> Self {
        value.0
    }
}

impl From<Vec<String>> for FileExtensions {
    fn from(value: Vec<String>) -> Self {
        Self(value)
    }
}

impl From<FileType> for FileExtensions {
    fn from(file_type: FileType) -> Self {
        match file_type {
            FileType::Html => FileType::html_extensions(),
            FileType::Markdown => FileType::markdown_extensions(),
            FileType::Plaintext => FileType::plaintext_extensions(),
        }
    }
}

impl FromIterator<String> for FileExtensions {
    fn from_iter<T: IntoIterator<Item = String>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl Iterator for FileExtensions {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.pop()
    }
}

impl std::str::FromStr for FileExtensions {
    type Err = std::convert::Infallible; // Cannot fail parsing

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.split(',').map(String::from).collect()))
    }
}

/// `FileType` defines which file types lychee can handle
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum FileType {
    /// File in HTML format
    Html,
    /// File in Markdown format
    Markdown,
    /// Generic text file without syntax-specific parsing
    Plaintext,
}

impl FileType {
    /// All known Markdown extensions
    const MARKDOWN_EXTENSIONS: &'static [&'static str] = &[
        "markdown", "mkdown", "mkdn", "mdwn", "mdown", "mdx", "mkd", "md",
    ];

    /// All known HTML extensions
    const HTML_EXTENSIONS: &'static [&'static str] = &["htm", "html"];

    /// All known plaintext extensions
    const PLAINTEXT_EXTENSIONS: &'static [&'static str] = &["txt"];

    /// Default extensions which are checked by lychee
    #[must_use]
    pub fn default_extensions() -> FileExtensions {
        let mut extensions = FileExtensions::empty();
        extensions.extend(Self::markdown_extensions());
        extensions.extend(Self::html_extensions());
        extensions.extend(Self::plaintext_extensions());
        extensions
    }

    /// All known Markdown extensions
    #[must_use]
    pub fn markdown_extensions() -> FileExtensions {
        Self::MARKDOWN_EXTENSIONS
            .iter()
            .map(|&s| s.to_string())
            .collect()
    }

    /// All known HTML extensions
    #[must_use]
    pub fn html_extensions() -> FileExtensions {
        Self::HTML_EXTENSIONS
            .iter()
            .map(|&s| s.to_string())
            .collect()
    }

    /// All known plaintext extensions
    #[must_use]
    pub fn plaintext_extensions() -> FileExtensions {
        Self::PLAINTEXT_EXTENSIONS
            .iter()
            .map(|&s| s.to_string())
            .collect()
    }

    /// Get the [`FileType`] from an extension string
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        let ext = extension.to_lowercase();
        if Self::MARKDOWN_EXTENSIONS.contains(&ext.as_str()) {
            Some(Self::Markdown)
        } else if Self::HTML_EXTENSIONS.contains(&ext.as_str()) {
            Some(Self::Html)
        } else if Self::PLAINTEXT_EXTENSIONS.contains(&ext.as_str()) {
            Some(Self::Plaintext)
        } else {
            None
        }
    }
}

impl Default for FileType {
    fn default() -> Self {
        // This is the default file type when no other type can be determined.
        // It represents a generic text file with no specific syntax.
        Self::Plaintext
    }
}

impl<P: AsRef<Path>> From<P> for FileType {
    fn from(p: P) -> FileType {
        let path = p.as_ref();
        match path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .map(str::to_lowercase)
            .as_deref()
            .and_then(FileType::from_extension)
        {
            Some(file_type) => file_type,
            None if is_url(path) => FileType::Html,
            _ => FileType::default(),
        }
    }
}

/// Helper function to check if a path is likely a URL.
fn is_url(path: &Path) -> bool {
    path.to_str()
        .and_then(|s| Url::parse(s).ok())
        .is_some_and(|url| url.scheme() == "http" || url.scheme() == "https")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension() {
        assert_eq!(FileType::from("foo.md"), FileType::Markdown);
        assert_eq!(FileType::from("foo.MD"), FileType::Markdown);
        assert_eq!(FileType::from("foo.mdx"), FileType::Markdown);

        // Test that a file without an extension is considered plaintext
        assert_eq!(FileType::from("README"), FileType::Plaintext);
        assert_eq!(FileType::from("test"), FileType::Plaintext);

        assert_eq!(FileType::from("test.unknown"), FileType::Plaintext);
        assert_eq!(FileType::from("test.txt"), FileType::Plaintext);
        assert_eq!(FileType::from("README.TXT"), FileType::Plaintext);

        assert_eq!(FileType::from("test.htm"), FileType::Html);
        assert_eq!(FileType::from("index.html"), FileType::Html);
        assert_eq!(FileType::from("http://foo.com/index.html"), FileType::Html);
    }

    #[test]
    fn test_default_extensions() {
        let extensions = FileType::default_extensions();
        // Test some known extensions
        assert!(extensions.contains("md"));
        assert!(extensions.contains("html"));
        assert!(extensions.contains("markdown"));
        assert!(extensions.contains("htm"));
        // Test that the count matches our static arrays
        let all_extensions: Vec<_> = extensions.into();
        assert_eq!(
            all_extensions.len(),
            FileType::MARKDOWN_EXTENSIONS.len()
                + FileType::HTML_EXTENSIONS.len()
                + FileType::PLAINTEXT_EXTENSIONS.len()
        );
    }

    #[test]
    fn test_is_url() {
        // Valid URLs
        assert!(is_url(Path::new("http://foo.com")));
        assert!(is_url(Path::new("https://foo.com")));
        assert!(is_url(Path::new("http://www.foo.com")));
        assert!(is_url(Path::new("https://www.foo.com")));
        assert!(is_url(Path::new("http://foo.com/bar")));
        assert!(is_url(Path::new("https://foo.com/bar")));
        assert!(is_url(Path::new("http://foo.com:8080")));
        assert!(is_url(Path::new("https://foo.com:8080")));
        assert!(is_url(Path::new("http://foo.com/bar?q=hello")));
        assert!(is_url(Path::new("https://foo.com/bar?q=hello")));

        // Invalid URLs
        assert!(!is_url(Path::new("foo.com")));
        assert!(!is_url(Path::new("www.foo.com")));
        assert!(!is_url(Path::new("foo")));
        assert!(!is_url(Path::new("foo/bar")));
        assert!(!is_url(Path::new("foo/bar/baz")));
        assert!(!is_url(Path::new("file:///foo/bar.txt")));
        assert!(!is_url(Path::new("ftp://foo.com")));
    }

    #[test]
    fn test_from_extension() {
        // Valid extensions
        assert_eq!(FileType::from_extension("html"), Some(FileType::Html));
        assert_eq!(FileType::from_extension("HTML"), Some(FileType::Html));
        assert_eq!(FileType::from_extension("htm"), Some(FileType::Html));
        assert_eq!(
            FileType::from_extension("markdown"),
            Some(FileType::Markdown)
        );
        assert_eq!(FileType::from_extension("md"), Some(FileType::Markdown));
        assert_eq!(FileType::from_extension("MD"), Some(FileType::Markdown));
        assert_eq!(FileType::from_extension("txt"), Some(FileType::Plaintext));
        assert_eq!(FileType::from_extension("TXT"), Some(FileType::Plaintext));

        // Unknown extension
        assert_eq!(FileType::from_extension("unknown"), None);
        assert_eq!(FileType::from_extension("xyz"), None);
    }
}
