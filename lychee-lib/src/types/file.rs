use std::{fmt::Display, path::Path};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// `FileType` defines which file types lychee can handle
pub enum FileType {
    /// PDF file type
    #[cfg(feature = "pdf")]
    Pdf,
    /// EPUB file type
    #[cfg(feature = "epub")]
    Epub,
    /// File in HTML format
    Html,
    /// File in Markdown format
    Markdown,
    /// Generic text file without syntax-specific parsing
    Plaintext,
}

impl Default for FileType {
    fn default() -> Self {
        Self::Plaintext
    }
}

impl Display for FileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "pdf")]
            FileType::Pdf => write!(f, "pdf"),
            #[cfg(feature = "epub")]
            FileType::Epub => write!(f, "epub"),
            FileType::Html => write!(f, "html"),
            FileType::Markdown => write!(f, "md"),
            FileType::Plaintext => write!(f, "txt"),
        }
    }
}

impl<P: AsRef<Path>> From<P> for FileType {
    /// Detect if the given path points to a Markdown, HTML, or plaintext file.
    fn from(p: P) -> FileType {
        let path = p.as_ref();
        // Assume HTML in case of no extension.
        // Note: this is only reasonable for URLs; not paths on disk.
        // For example, `README` without an extension is more likely to be a plaintext file.
        // A better solution would be to also implement `From<Url> for FileType`.
        // Unfortunately that's not possible without refactoring, as
        // `AsRef<Path>` could be implemented for `Url` in the future, which is why
        // `From<Url> for FileType` is not allowed.
        // As a workaround, we check if we got a known web-protocol
        let is_url = path.starts_with("http");

        match path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .map(str::to_lowercase)
            .as_deref()
        {
            Some("md" | "markdown") => FileType::Markdown,
            Some("htm" | "html") => FileType::Html,
            #[cfg(feature = "pdf")]
            Some("pdf") => FileType::Pdf,
            #[cfg(feature = "epub")]
            Some("epub") => FileType::Epub,
            None if is_url => FileType::Html,
            _ => FileType::Plaintext,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension() {
        assert_eq!(FileType::from(Path::new("foo.md")), FileType::Markdown);
        assert_eq!(FileType::from(Path::new("foo.MD")), FileType::Markdown);

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

        assert_eq!(FileType::from(Path::new("foo.pdf")), FileType::Pdf);
        assert_eq!(FileType::from(Path::new("foo.epub")), FileType::Epub);
    }
}
