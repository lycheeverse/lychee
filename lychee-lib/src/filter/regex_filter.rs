use regex::RegexSet;

/// Filter configuration for the link checker.
/// You can include and exclude links and paths based on regex patterns
#[derive(Clone, Debug)]
pub struct RegexFilter {
    /// User-defined set of regex patterns
    pub regex: RegexSet,
}

impl RegexFilter {
    #[inline]
    #[must_use]
    /// Returns `true` if the given input string matches the regex set
    /// and should hence be included or excluded
    pub fn is_match(&self, input: &str) -> bool {
        self.regex.is_match(input)
    }

    #[inline]
    #[must_use]
    /// Whether there were no regular expressions defined
    pub fn is_empty(&self) -> bool {
        self.regex.is_empty()
    }

    /// Create a new empty regex set.
    pub fn empty() -> Self {
        Self {
            regex: RegexSet::empty(),
        }
    }
}

impl PartialEq for RegexFilter {
    fn eq(&self, other: &Self) -> bool {
        // Workaround, see https://github.com/rust-lang/regex/issues/364
        self.regex.patterns() == other.regex.patterns()
    }
}

impl From<RegexSet> for RegexFilter {
    fn from(regex: RegexSet) -> Self {
        Self { regex }
    }
}
