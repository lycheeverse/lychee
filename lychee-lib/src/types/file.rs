use ignore::types::TypesBuilder;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display, Formatter},
    hash::Hash,
    path::Path,
};
use url::Url;

/// File types that can be checked with lychee
///
/// The file type is determined by the file extension
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileType {
    /// HTML files (e.g. `*.html`, `*.htm`)
    Html,
    /// Markdown files (e.g. `*.md`, `*.markdown`)
    Markdown,
    /// Plaintext files (e.g. `*.txt` or files without an extension
    /// if they are not URLs)
    Plaintext,
}

impl FileType {
    /// All known Markdown extensions
    const MARKDOWN_EXTENSIONS: &'static str = "*.{md,markdown,mkdown,mkdn,mdwn,mdown,mdx,mkd}";
    /// All known HTML extensions
    const HTML_EXTENSIONS: &'static str = "*.{html,htm}";

    const fn as_ignore_type(self) -> Option<(&'static str, &'static str)> {
        match self {
            FileType::Markdown => Some(("markdown", Self::MARKDOWN_EXTENSIONS)),
            FileType::Html => Some(("html", Self::HTML_EXTENSIONS)),
            FileType::Plaintext => None,
        }
    }

    /// Convert to [`ignore::Types`] for file matching
    pub(crate) fn to_ignore_types(self) -> ignore::types::Types {
        let mut builder = ignore::types::TypesBuilder::new();

        // Add the appropriate patterns based on the file type
        match self {
            FileType::Markdown => {
                builder
                    .add("markdown", "*.{md,markdown,mkdown,mkdn,mdwn,mdown,mdx,mkd}")
                    .unwrap();
            }
            FileType::Html => {
                builder.add("html", "*.{html,htm}").unwrap();
            }
            FileType::Plaintext => {
                // Match any file for plaintext
                builder.add("any", "*").unwrap();
            }
        }

        builder.select("all");
        builder.build().unwrap()
    }

    /// Check if the file type matches the given path
    ///
    /// # Panics
    ///  
    /// If `name` is `all` or otherwise contains any character that is not a
    /// Unicode letter or number, then this function will panic. Since we are
    /// using a hardcoded string, this should never happen.
    #[must_use]
    pub fn matches(&self, path: &Path) -> bool {
        // URLs are always treated as HTML
        if is_url(path) {
            return *self == FileType::Html;
        }

        // Build ignore::Types matcher for the specific type
        let Some((name, pattern)) = self.as_ignore_type() else {
            return *self == FileType::Plaintext;
        };

        let mut builder = TypesBuilder::new();
        builder.add(name, pattern).unwrap();
        builder.select("all");
        let types = builder.build().unwrap();

        types.matched(path, true).is_whitelist()
    }

    /// Get the default extensions for each file type
    /// in the order they should be checked
    #[must_use]
    pub fn default_extensions() -> Vec<Self> {
        vec![Self::Markdown, Self::Html]
    }
}

impl Default for FileType {
    fn default() -> Self {
        Self::Plaintext
    }
}

impl<P: AsRef<Path>> From<P> for FileType {
    fn from(p: P) -> FileType {
        let path = p.as_ref();
        if is_url(path) {
            return FileType::Html;
        }

        // Try each type in order
        for ty in [FileType::Markdown, FileType::Html] {
            if ty.matches(path) {
                return ty;
            }
        }
        FileType::Plaintext
    }
}

impl Display for FileType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            FileType::Html => write!(f, "html"),
            FileType::Markdown => write!(f, "markdown"),
            FileType::Plaintext => write!(f, "plaintext"),
        }
    }
}

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
    fn test_from_path() {
        assert_eq!(FileType::from(Path::new("foo.md")), FileType::Markdown);
        assert_eq!(FileType::from(Path::new("foo.MD")), FileType::Markdown);
        assert_eq!(FileType::from(Path::new("foo.mdx")), FileType::Markdown);
        assert_eq!(
            FileType::from(Path::new("test.unknown")),
            FileType::Plaintext
        );
        assert_eq!(FileType::from(Path::new("test.htm")), FileType::Html);
        assert_eq!(FileType::from(Path::new("index.html")), FileType::Html);
        assert_eq!(
            FileType::from(Path::new("http://foo.com/index.html")),
            FileType::Html
        );
    }

    #[test]
    fn test_matches() {
        assert!(FileType::Markdown.matches(Path::new("test.md")));
        assert!(FileType::Html.matches(Path::new("test.html")));
        assert!(!FileType::Markdown.matches(Path::new("test.html")));
        assert!(!FileType::Html.matches(Path::new("test.md")));
        assert!(FileType::Plaintext.matches(Path::new("test.txt")));
    }
}
