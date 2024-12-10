use std::path::Path;
use url::Url;

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

impl Default for FileType {
    fn default() -> Self {
        Self::Plaintext
    }
}

impl<P: AsRef<Path>> From<P> for FileType {
    /// Detect if the given path points to a Markdown, HTML, or plaintext file.
    //
    // Assume HTML in case of no extension.
    //
    // This is only reasonable for URLs, not paths on disk. For example,
    // a file named `README` without an extension is more likely to be a
    // plaintext file.
    //
    // A better solution would be to also implement `From<Url> for
    // FileType`. Unfortunately that's not possible without refactoring, as
    // `AsRef<Path>` could be implemented for `Url` in the future, which is
    // why `From<Url> for FileType` is not allowed (orphan rule).
    //
    // As a workaround, we check if the scheme is `http` or `https` and
    // assume HTML in that case.
    fn from(p: P) -> FileType {
        let path = p.as_ref();
        match path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .map(str::to_lowercase)
            .as_deref()
        {
            // https://superuser.com/a/285878
            Some("markdown" | "mkdown" | "mkdn" | "mdwn" | "mdown" | "mdx" | "mkd" | "md") => {
                FileType::Markdown
            }
            Some("htm" | "html") => FileType::Html,
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
