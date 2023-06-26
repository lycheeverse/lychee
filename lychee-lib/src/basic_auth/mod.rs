use regex::RegexSet;
use thiserror::Error;

use crate::{BasicAuthCredentials, BasicAuthSelector, Uri};

#[derive(Debug, Error)]
pub enum BasicAuthExtractorError {
    #[error("RegexSet error")]
    RegexSetError(#[from] regex::Error),
}

/// Extracts basic auth credentials from a given URI.
/// Credentials are extracted if the URI matches one of the provided
/// [`BasicAuthSelector`] instances.
#[derive(Debug, Clone)]
pub struct BasicAuthExtractor {
    credentials: Vec<BasicAuthCredentials>,
    regex_set: RegexSet,
}

impl BasicAuthExtractor {
    /// Creates a new [`BasicAuthExtractor`] from a list of [`BasicAuthSelector`]
    /// instances.
    ///
    /// # Errors
    ///
    /// Returns an error if the provided [`BasicAuthSelector`] instances contain
    /// invalid regular expressions.
    ///
    /// # Examples
    ///
    /// ```
    /// use lychee_lib::{BasicAuthExtractor, BasicAuthSelector};
    /// use std::str::FromStr;
    ///
    /// let selectors = vec![
    ///    BasicAuthSelector::from_str("http://example.com foo:bar").unwrap(),
    /// ];
    ///
    /// let extractor = BasicAuthExtractor::new(selectors).unwrap();
    /// ```
    pub fn new<T: AsRef<[BasicAuthSelector]>>(
        selectors: T,
    ) -> Result<Self, BasicAuthExtractorError> {
        let mut raw_uri_regexes = Vec::new();
        let mut credentials = Vec::new();

        for selector in selectors.as_ref() {
            raw_uri_regexes.push(selector.raw_uri_regex.clone());
            credentials.push(selector.credentials.clone());
        }

        let regex_set = RegexSet::new(raw_uri_regexes)?;

        Ok(Self {
            credentials,
            regex_set,
        })
    }

    /// Matches the provided URI against the [`RegexSet`] and returns
    /// [`BasicAuthCredentials`] if the a match was found. It should be noted
    /// that only the first match will be used to return the appropriate
    /// credentials.
    pub(crate) fn matches(&self, uri: &Uri) -> Option<BasicAuthCredentials> {
        let matches: Vec<_> = self.regex_set.matches(uri.as_str()).into_iter().collect();

        if matches.is_empty() {
            return None;
        }

        Some(self.credentials[matches[0]].clone())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_basic_auth_extractor_new() {
        let selector_str = "http://example.com foo:bar";
        let selector = BasicAuthSelector::from_str(selector_str).unwrap();
        let extractor = BasicAuthExtractor::new(vec![selector]).unwrap();

        assert_eq!(extractor.credentials.len(), 1);
        assert_eq!(extractor.credentials[0].username, "foo");
        assert_eq!(extractor.credentials[0].password, "bar");
    }

    #[test]
    fn test_basic_auth_extractor_matches() {
        let selector_str = "http://example.com foo:bar";
        let selector = BasicAuthSelector::from_str(selector_str).unwrap();
        let extractor = BasicAuthExtractor::new(vec![selector]).unwrap();

        let uri = Uri::try_from("http://example.com").unwrap();
        let credentials = extractor.matches(&uri).unwrap();

        assert_eq!(credentials.username, "foo");
        assert_eq!(credentials.password, "bar");
    }

    #[test]
    fn test_basic_auth_extractor_no_match() {
        let selector_str = "http://example.com foo:bar";
        let selector = BasicAuthSelector::from_str(selector_str).unwrap();
        let extractor = BasicAuthExtractor::new(vec![selector]).unwrap();

        let uri = Uri::try_from("http://test.com").unwrap();
        let credentials = extractor.matches(&uri);

        assert!(credentials.is_none());
    }
}
