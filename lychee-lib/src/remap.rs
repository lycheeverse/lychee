use std::ops::Index;

use crate::Result;
use regex::Regex;
use reqwest::Url;

use crate::Uri;

/// Remaps allow mapping from a URI pattern to a different URI
///
/// Some use-cases are
/// - Testing URIs prior to production deployment
/// - Testing URIs behind a proxy
///
/// Be careful when using this feature because checking every link against a
/// large set of regular expressions has a performance impact. Also there are no
/// constraints on the URI mapping, so the rules might contradict with each
/// other.
#[derive(Debug, Clone)]
pub struct Remaps(Vec<(Regex, Url)>);

impl Remaps {
    /// Create a new remapper
    #[must_use]
    pub fn new(patterns: Vec<(Regex, Url)>) -> Self {
        Self(patterns)
    }

    /// Remap URI using the client-defined remap patterns
    ///
    /// # Errors
    ///
    /// Returns an error if the remapping value is not a URI
    pub fn remap(&self, uri: Uri) -> Result<Uri> {
        let mut uri = uri;
        for (pattern, new_uri) in &self.0 {
            if pattern.is_match(uri.as_str()) {
                uri = Uri::try_from(new_uri.clone())?;
            }
        }
        Ok(uri)
    }

    /// Returns `true` if there are no remappings defined.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get the number of defined remap rules
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl Index<usize> for Remaps {
    type Output = (Regex, Url);

    fn index(&self, index: usize) -> &(regex::Regex, url::Url) {
        &self.0[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remap() {
        let pattern = Regex::new("https://example.com").unwrap();
        let uri = Uri::try_from("http://127.0.0.1:8080").unwrap();
        let remaps = Remaps::new(vec![(pattern, uri.clone().url)]);

        let input = Uri::try_from("https://example.com").unwrap();
        let remapped = remaps.remap(input).unwrap();

        assert_eq!(remapped, uri);
    }

    #[test]
    fn test_remap_path() {
        let pattern = Regex::new("../../issues").unwrap();
        let uri = Uri::try_from("https://example.com").unwrap();
        let remaps = Remaps::new(vec![(pattern, uri.clone().url)]);

        let input = Uri::try_from("../../issues").unwrap();
        let remapped = remaps.remap(input).unwrap();

        assert_eq!(remapped, uri);
    }
}
