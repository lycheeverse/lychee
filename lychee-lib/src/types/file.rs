use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display, Formatter},
    path::Path,
    str::FromStr,
};
use url::Url;

/// File extensions that lychee can handle
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileExtensions(Vec<String>);

impl FileExtensions {
    /// Create a new set of file extensions
    #[must_use]
    pub const fn new(extensions: Vec<String>) -> Self {
        Self(extensions)
    }

    /// Check if a file extension is supported
    #[must_use]
    pub fn is_supported(&self, extension: &str) -> bool {
        self.0.contains(&extension.to_string())
    }
}

#[cfg(test)]
impl FileExtensions {
    /// Get the number of supported file extensions
    ///
    /// This is only used in tests to verify that the number of supported
    /// extensions matches the expected number.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if the set of file extensions is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<FileExtensions> for ignore::types::Types {
    fn from(extensions: FileExtensions) -> Self {
        let mut types_builder = ignore::types::TypesBuilder::new();
        for ext in extensions.0 {
            types_builder.add(&ext, &format!("*.{ext}")).unwrap();
        }
        types_builder.select("all").build().unwrap()
    }
}

impl From<Vec<String>> for FileExtensions {
    fn from(extensions: Vec<String>) -> Self {
        Self(extensions)
    }
}

impl FromStr for FileExtensions {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.split(',').map(str::to_string).collect()))
    }
}

impl Display for FileExtensions {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.0.join(","))
    }
}

impl Default for FileExtensions {
    fn default() -> Self {
        // Check Markdown and HTML files by default
        FileType::default_extensions()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// `FileType` defines which file types lychee can handle
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

    /// Default extensions which are supported by lychee
    #[must_use]
    pub fn default_extensions() -> FileExtensions {
        let mut extensions = Vec::new();
        extensions.extend(Self::MARKDOWN_EXTENSIONS.iter().map(|&s| s.to_string()));
        extensions.extend(Self::HTML_EXTENSIONS.iter().map(|&s| s.to_string()));
        FileExtensions(extensions)
    }

    /// Get the [`FileType`] from an extension string
    fn from_extension(ext: &str) -> Option<Self> {
        let ext = ext.to_lowercase();
        if Self::MARKDOWN_EXTENSIONS.contains(&ext.as_str()) {
            Some(Self::Markdown)
        } else if Self::HTML_EXTENSIONS.contains(&ext.as_str()) {
            Some(Self::Html)
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
        .map_or(false, |url| {
            url.scheme() == "http" || url.scheme() == "https"
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension() {
        assert_eq!(FileType::from(Path::new("foo.md")), FileType::Markdown);
        assert_eq!(FileType::from(Path::new("foo.MD")), FileType::Markdown);
        assert_eq!(FileType::from(Path::new("foo.mdx")), FileType::Markdown);

        assert_eq!(
            FileType::from(Path::new("test.unknown")),
            FileType::Plaintext
        );
        assert_eq!(FileType::from(Path::new("test")), FileType::Plaintext);
        assert_eq!(FileType::from(Path::new("test.txt")), FileType::Plaintext);
        assert_eq!(FileType::from(Path::new("README.TXT")), FileType::Plaintext);

        assert_eq!(FileType::from(Path::new("test.htm")), FileType::Html);
        assert_eq!(FileType::from(Path::new("index.html")), FileType::Html);
        assert_eq!(
            FileType::from(Path::new("http://foo.com/index.html")),
            FileType::Html
        );
    }

    #[test]
    fn test_default_extensions() {
        let extensions = FileType::default_extensions();
        // Test some known extensions
        assert!(extensions.is_supported("md"));
        assert!(extensions.is_supported("html"));
        assert!(extensions.is_supported("markdown"));
        assert!(extensions.is_supported("htm"));
        // Test the count matches our static arrays
        assert_eq!(
            extensions.len(),
            FileType::MARKDOWN_EXTENSIONS.len() + FileType::HTML_EXTENSIONS.len()
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
}
