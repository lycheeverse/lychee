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
    /// Returns `true` if the given input string matches the regex set
    /// and should hence be included and checked
    pub fn is_match(&self, input: &str) -> bool {
        self.regex.is_match(input)
    }

    #[inline]
    #[must_use]
    /// Whether there were no regular expressions defined for inclusion
    pub fn is_empty(&self) -> bool {
        self.regex.is_empty()
    }
}
