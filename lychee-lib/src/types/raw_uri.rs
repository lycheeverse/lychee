use std::fmt::Display;

/// A way to classify links to make it easier to offer fine control over the
/// links that will be checked
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum UriKind {
    /// Normal web-link that gets rendered as a hyperlink
    Strict,
    /// Link occuring in non-human-clickable sections like comments, `<code>`,
    /// or `<pre>` tags
    Fuzzy,
    /// The visibility of the link cannot be inferred during parsing
    /// This can be the case when a link gets created from a raw string
    Unknown,
}

/// A raw URI that got extracted from a document with a fuzzy parser.
/// Note that this can still be invalid according to stricter URI standards
#[derive(Clone, Debug, PartialEq)]
pub struct RawUri {
    pub text: String,
    pub kind: UriKind,
}

impl RawUri {
    // Taken from https://github.com/getzola/zola/blob/master/components/link_checker/src/lib.rs
    pub(crate) fn is_anchor(&self) -> bool {
        self.text.starts_with('#')
    }
}
impl Display for RawUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({:?})", self.text, self.kind)
    }
}
