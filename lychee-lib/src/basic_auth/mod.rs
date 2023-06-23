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
    pub fn new(selectors: Vec<BasicAuthSelector>) -> Result<Self, BasicAuthExtractorError> {
        let mut raw_uri_regexes = Vec::new();
        let mut credentials = Vec::new();

        for selector in selectors {
            raw_uri_regexes.push(selector.raw_uri_regex);
            credentials.push(selector.credentials);
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
