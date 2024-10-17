use std::fmt::Display;

/// A raw URI that got extracted from a document with a fuzzy parser.
/// Note that this can still be invalid according to stricter URI standards
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawUri {
    /// Unparsed URI represented as a `String`. There is no guarantee that it
    /// can be parsed into a URI object
    pub text: String,
    /// Name of the element that contained the URI (e.g. `a` for the <a> tag).
    /// This is a way to classify links to make it easier to offer fine control
    /// over the links that will be checked e.g. by trying to filter out links
    /// that were found in unwanted tags like `<pre>` or `<code>`.
    pub element: Option<String>,
    /// Name of the attribute that contained the URI (e.g. `src`). This is a way
    /// to classify links to make it easier to offer fine control over the links
    /// that will be checked e.g. by trying to filter out links that were found
    /// in unwanted attributes like `srcset` or `manifest`.
    pub attribute: Option<String>,
}

impl Display for RawUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (Attribute: {:?})", self.text, self.attribute)
    }
}

impl From<&str> for RawUri {
    fn from(text: &str) -> Self {
        RawUri {
            text: text.to_string(),
            element: None,
            attribute: None,
        }
    }
}
