use regex::RegexSet;

/// Include configuration for the link checker.
/// You can include links based on regex patterns
#[derive(Clone, Debug)]
pub struct Includes {
    pub regex: Option<RegexSet>,
}

impl Default for Includes {
    fn default() -> Self {
        Self { regex: None }
    }
}

impl Includes {
    pub fn regex(&self, input: &str) -> bool {
        if let Some(includes) = &self.regex {
            if includes.is_match(input) {
                return true;
            }
        }
        false
    }

    pub fn is_empty(&self) -> bool {
        match &self.regex {
            None => true,
            Some(regex_set) => regex_set.is_empty(),
        }
    }
}
