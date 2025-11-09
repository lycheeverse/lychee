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
    pub const fn new(patterns: Vec<(Regex, String)>) -> Self {
        Self(patterns)
    }

    /// Returns an iterator over the rules.
    // `iter_mut` is deliberately avoided.
    pub fn iter(&self) -> std::slice::Iter<'_, (Regex, String)> {
        self.0.iter()
    }

    /// Remap URL against remapping rules.
    ///
    /// If there is no matching rule, the original URL is returned.
    ///
    /// # Errors
    ///
    /// Returns an `Err` if the remapping rule produces an invalid URL.
    #[must_use = "Remapped URLs must be used"]
    pub fn remap(&self, original: &Url) -> Result<Url> {
        for (pattern, replacement) in self {
            if pattern.is_match(original.as_str()) {
                let after = pattern.replace_all(original.as_str(), replacement);
                let after_url = Url::parse(after.as_ref()).map_err(|_| {
                    ErrorKind::InvalidUrlRemap(format!(
                        "The remapping pattern must produce a valid URL, but it is not: {after}"
                    ))
                })?;
                return Ok(after_url);
            }
        }
        Ok(original.clone())
    }

    /// Returns `true` if there is no remapping rule defined.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get the number of remapping rules.
    #[must_use]
    pub const fn len(&self) -> usize {
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
                return Err(ErrorKind::InvalidUrlRemap(format!(
                    "Cannot parse into URI remapping, must be a Regex pattern and a URL separated by whitespaces: {remap}"
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
        let input = "https://example.com";
        let input_url = Url::try_from(input).unwrap();
        let input_pattern = Regex::new(input).unwrap();
        let replacement = "http://127.0.0.1:8080";
        let remaps = Remaps::new(vec![(input_pattern, replacement.to_string())]);

        let output = remaps.remap(&input_url).unwrap();

        assert_eq!(output, Url::try_from(replacement).unwrap());
    }

    #[test]
    fn test_remap_path() {
        let input = Url::try_from("file://../../issues").unwrap();
        let input_pattern = Regex::new(".*?../../issues").unwrap();
        let replacement = Url::try_from("https://example.com").unwrap();
        let remaps = Remaps::new(vec![(input_pattern, replacement.to_string())]);

        let output = remaps.remap(&input).unwrap();

        assert_eq!(output, replacement);
    }

    #[test]
    fn test_remap_skip() {
        let input = Url::try_from("https://unrelated.example.com").unwrap();
        let pattern = Regex::new("https://example.com").unwrap();
        let replacement = Url::try_from("http://127.0.0.1:8080").unwrap();
        let remaps = Remaps::new(vec![(pattern, replacement.to_string())]);

        let output = remaps.remap(&input).unwrap();

        // URL was not modified
        assert_eq!(input, output);
    }

    #[test]
    fn test_remap_url_to_file() {
        let pattern = Regex::new("https://docs.example.org").unwrap();
        let replacement = "file:///Users/user/code/repo/docs/_site";
        let remaps = Remaps::new(vec![(pattern, replacement.to_string())]);

        let tests = [
            (
                "https://docs.example.org/integrations/distcp.html",
                "file:///Users/user/code/repo/docs/_site/integrations/distcp.html",
            ),
            (
                "https://docs.example.org/howto/import.html#working-with-imported-data",
                "file:///Users/user/code/repo/docs/_site/howto/import.html#working-with-imported-data",
            ),
            (
                "https://docs.example.org/howto/garbage-collection-committed.html",
                "file:///Users/user/code/repo/docs/_site/howto/garbage-collection-committed.html",
            ),
        ];

        for (input, expected) in tests {
            let input = Url::parse(input).unwrap();
            let output = remaps.remap(&input).unwrap();
            assert_eq!(output, Url::parse(expected).unwrap());
        }
    }

    /// This is a partial remap, i.e. the URL is not fully replaced but only
    /// part of it. The parts to be replaced are defined by the regex pattern
    /// using capture groups.
    #[test]
    fn test_remap_capture_group() {
        let input = Url::try_from("https://example.com/1/2/3").unwrap();
        let input_pattern = Regex::new("https://example.com/.*?/(.*?)/.*").unwrap();
        let replacement = Url::try_from("https://example.com/foo/$1/bar").unwrap();

        let remaps = Remaps::new(vec![(input_pattern, replacement.to_string())]);

        let output = remaps.remap(&input).unwrap();

        assert_eq!(
            output,
            Url::try_from("https://example.com/foo/2/bar").unwrap()
        );
    }

    #[test]
    fn test_remap_named_capture() {
        let input = Url::try_from("https://example.com/1/2/3").unwrap();
        let input_pattern = Regex::new("https://example.com/.*?/(?P<foo>.*?)/.*").unwrap();
        let replacement = Url::try_from("https://example.com/foo/$foo/bar").unwrap();

        let remaps = Remaps::new(vec![(input_pattern, replacement.to_string())]);

        let output = remaps.remap(&input).unwrap();

        assert_eq!(
            output,
            Url::try_from("https://example.com/foo/2/bar").unwrap()
        );
    }

    #[test]
    fn test_remap_named_capture_shorthand() {
        let input = Url::try_from("https://example.com/1/2/3").unwrap();
        #[allow(clippy::invalid_regex)]
        // Clippy acts up here, but this syntax is actually valid
        // See https://docs.rs/regex/latest/regex/index.html#grouping-and-flags
        let input_pattern = Regex::new(r"https://example.com/.*?/(?<foo>.*?)/.*").unwrap();
        let replacement = Url::try_from("https://example.com/foo/$foo/bar").unwrap();

        let remaps = Remaps::new(vec![(input_pattern, replacement.to_string())]);

        let output = remaps.remap(&input).unwrap();

        assert_eq!(
            output,
            Url::try_from("https://example.com/foo/2/bar").unwrap()
        );
    }
}
