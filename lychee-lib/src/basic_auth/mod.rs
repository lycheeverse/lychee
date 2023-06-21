use regex::RegexSet;
use thiserror::Error;

use crate::{BasicAuthCredentials, BasicAuthSelector, Uri};

#[derive(Debug, Error)]
pub enum BasicAuthExtractorError {
    #[error("RegexSet error")]
    RegexSetError(#[from] regex::Error),
}

#[derive(Debug, Clone)]
pub(crate) struct BasicAuthExtractor {
    credentials: Vec<BasicAuthCredentials>,
    regex_set: RegexSet,
}

impl BasicAuthExtractor {
    pub(crate) fn new(selectors: Vec<BasicAuthSelector>) -> Result<Self, BasicAuthExtractorError> {
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
