//! Remapping rules which allow to map URLs matching a pattern to a different
//! URL.
//!
//! # Notes
//! Use in moderation as there are no sanity or performance guarantees.
//!
//! - There is no constraint on remapping rules upon instantiation or during
//!   remapping. In particular, rules are checked sequentially so later rules
//!   might contradict with earlier ones if they both match a URL.
//! - A large rule set has a performance impact because the client needs to
//!   match every link against all rules.

// Notes on terminology:
// The major difference between URI (Uniform Resource Identifier) and
// URL (Uniform Resource Locator) is that the former is an identifier for
// resources and the latter is a locator.
// We are not interested in differentiating resources by names and the purpose of
// remapping is to provide an alternative **location** in certain
// circumanstances. Thus the documentation should be about remapping URLs
// (locations), not remapping URIs (identities).

use std::ops::Index;

use regex::Regex;
use url::Url;

use crate::{ErrorKind, Result};

/// Rules that remap matching URL patterns.
///
/// Some use-cases are:
/// - Testing URLs prior to production deployment.
/// - Testing URLs behind a proxy.
///
/// # Notes
/// See module level documentation of usage notes.
#[derive(Debug, Clone)]
pub struct Remaps(Vec<(Regex, String)>);

impl Remaps {
    /// Create a new remapper
    #[must_use]
    pub fn new(patterns: Vec<(Regex, String)>) -> Self {
        Self(patterns)
    }

    /// Returns an iterator over the rules.
    // `iter_mut` is deliberately avoided.
    pub fn iter(&self) -> std::slice::Iter<(Regex, String)> {
        self.0.iter()
    }

    /// Remap URL against remapping rules.
    pub fn remap(&self, url: &mut Url) -> Result<()> {
        for (pattern, replacement) in self {
            if pattern.is_match(url.as_str()) {
                // *url = new_url.clone();

                let after = pattern.replace_all(url.as_str(), replacement.as_str());
                *url = Url::parse(after.as_ref()).map_err(|_| {
                    ErrorKind::InvalidUrlRemap(format!("The remapped URL is invalid: {after}"))
                })?;
                return Ok(());
            }
        }
        Ok(())
    }

    /// Returns `true` if there is no remapping rule defined.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get the number of remapping rules.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl Index<usize> for Remaps {
    type Output = (Regex, String);

    fn index(&self, index: usize) -> &(regex::Regex, String) {
        &self.0[index]
    }
}

impl TryFrom<&[String]> for Remaps {
    type Error = ErrorKind;

    /// Try to convert a slice of `String`s to remapping rules.
    ///
    /// Each string should contain a Regex pattern and a URL, separated by
    /// whitespaces.
    ///
    /// # Errors
    ///
    /// Returns an `Err` if:
    /// - Any string in the slice is not of the form `REGEX URL`.
    /// - REGEX is not a valid regular expression.
    /// - URL is not a valid URL.
    fn try_from(remaps: &[String]) -> std::result::Result<Self, Self::Error> {
        let mut parsed = Vec::new();

        for remap in remaps {
            let params: Vec<_> = remap.split_whitespace().collect();
            if params.len() != 2 {
                return Err(ErrorKind::InvalidUrlRemap(
                    format!("Cannot parse into URI remapping, must be a Regex pattern and a URL separated by whitespaces: {remap}"
                    )));
            }

            let pattern = Regex::new(params[0])?;
            let replacement = params[1].to_string();
            parsed.push((pattern, replacement));
        }

        Ok(Remaps::new(parsed))
    }
}

// Implementation for mutable iterator and moving iterator are deliberately
// avoided
impl<'a> IntoIterator for &'a Remaps {
    type Item = &'a (Regex, String);

    type IntoIter = std::slice::Iter<'a, (Regex, String)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::*;

    #[test]
    fn test_remap() {
        let pattern = Regex::new("https://example.com").unwrap();
        let replacement = "http://127.0.0.1:8080".to_string();
        let remaps = Remaps::new(vec![(pattern, replacement.clone())]);

        let mut input = Url::try_from("https://example.com").unwrap();
        remaps.remap(&mut input).unwrap();

        assert_eq!(input, Url::try_from(replacement.as_str()).unwrap());
    }

    #[test]
    fn test_remap_path() {
        let pattern = Regex::new("../../issues").unwrap();
        let new_url = Url::try_from("https://example.com").unwrap();
        let remaps = Remaps::new(vec![(pattern, new_url.to_string())]);

        let mut input = Url::try_from("file://../../issues").unwrap();
        remaps.remap(&mut input).unwrap();

        assert_eq!(input, new_url);
    }

    // #[test]
    // fn test_remap_skip() {
    //     let pattern = Regex::new("https://example.com").unwrap();
    //     let new_url = Url::try_from("http://127.0.0.1:8080").unwrap();
    //     let remaps = Remaps::new(vec![(pattern, new_url)]);

    //     let mut input = Url::try_from("https://unrelated.example.com").unwrap();
    //     remaps.remap(&mut input);

    //     // URL was not modified
    //     assert_eq!(input, input);
    // }

    // fn test_remap_url_to_file() {
    //     let remap_source = Regex::new("https://docs.lakefs.io").unwrap();
    //     let remap_target = "file:///Users/rmoff/git/lakeFS/docs/_site";
    //     let remaps = Remaps::new(vec![(remap_source, remap_target)]);

    //     let tests = [
    //         (
    //             "https://docs.lakefs.io/integrations/distcp.html",
    //             "file:///Users/rmoff/git/lakeFS/docs/_site/integrations/distcp.html",
    //         ),
    //         (
    //             "https://docs.lakefs.io/howto/import.html#working-with-imported-data",
    //             "file:///Users/rmoff/git/lakeFS/docs/_site/howto/import.html#working-with-imported-data",
    //         ),
    //         (
    //             "https://docs.lakefs.io/howto/garbage-collection-committed.html",
    //             "file:///Users/rmoff/git/lakeFS/docs/_site/howto/garbage-collection-committed.html",
    //         ),
    //     ];

    //     for (input, expected) in tests.iter() {
    //         let mut input = Url::parse(*input).unwrap();
    //         remaps.remap(&mut input);
    //         assert_eq!(input, Url::parse(*expected).unwrap());
    //     }
    // }
}
