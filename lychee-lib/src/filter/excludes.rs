use regex::RegexSet;

/// Exclude configuration for the link checker.
/// You can ignore links based on regex patterns.
#[derive(Clone, Debug)]
pub struct Excludes {
    /// User-defined set of excluded regex patterns
    pub(crate) regex: RegexSet,
}

impl Excludes {
    #[inline]
    #[must_use]
    pub fn is_match(&self, input: &str) -> bool {
        self.regex.is_match(input)
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.regex.is_empty()
    }
}
