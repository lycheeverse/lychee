use regex::RegexSet;

/// Include configuration for the link checker.
/// You can include links based on regex patterns
#[derive(Clone, Debug)]
pub struct Includes {
    /// User-defined set of included regex patterns
    pub regex: RegexSet,
}

impl Includes {
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
