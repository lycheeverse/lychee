use regex::RegexSet;

/// Include configuration for the link checker.
/// You can include links based on regex patterns
#[derive(Clone, Debug, Default)]
pub struct Includes {
    pub regex: Option<RegexSet>,
}

impl Includes {
    #[inline]
    pub fn regex(&self, input: &str) -> bool {
        self.regex.as_ref().map_or(false, |re| re.is_match(input))
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.regex.as_ref().map_or(true, RegexSet::is_empty)
    }
}
