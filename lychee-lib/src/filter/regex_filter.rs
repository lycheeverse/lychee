use regex::RegexSet;

/// Filter configuration for the link checker.
/// You can include and exclude links and paths based on regex patterns
#[derive(Clone, Debug)]
pub struct RegexFilter {
    /// User-defined set of regex patterns
    regex: RegexSet,
}

impl RegexFilter {
    /// Create a new empty regex set.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            regex: RegexSet::empty(),
        }
    }

    /// Create a new empty regex set.
    pub fn new<I, S>(exprs: I) -> Result<Self, regex::Error>
    where
        S: AsRef<str>,
        I: IntoIterator<Item = S>,
    {
        Ok(Self {
            regex: RegexSet::new(exprs)?,
        })
    }

    #[inline]
    #[must_use]
    /// Returns `true` if the given input string matches the regex set
    /// and should hence be included or excluded
    pub fn is_match(&self, input: &str) -> bool {
        self.regex.is_match(input)
    }

    /// Returns the first regular expression that matches the input.
    #[must_use]
    pub fn matching_pattern(&self, input: &str) -> Option<&str> {
        self.regex
            .matches(input)
            .iter()
            .next()
            .and_then(|index| self.regex.patterns().get(index))
            .map(String::as_str)
    }

    #[inline]
    #[must_use]
    /// Whether there were no regular expressions defined
    pub fn is_empty(&self) -> bool {
        self.regex.is_empty()
    }
}

impl From<RegexSet> for RegexFilter {
    fn from(regex: RegexSet) -> Self {
        Self { regex }
    }
}

#[cfg(test)]
mod tests {
    use super::RegexFilter;

    #[test]
    fn finds_first_matching_pattern() {
        let filter = RegexFilter::new([r"example\.com", r"https://example"]).unwrap();

        assert_eq!(
            filter.matching_pattern("https://example.com"),
            Some(r"example\.com")
        );
        assert_eq!(filter.matching_pattern("https://other.org"), None);
    }
}
