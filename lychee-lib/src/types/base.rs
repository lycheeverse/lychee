use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

use crate::{ErrorKind, InputSource};

/// When encountering links without a domain in a document,
/// the base determines where this resource can be found.
///
/// This can be only be a remote URL.
/// For local paths, use `root-dir`
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
#[allow(variant_size_differences)]
#[serde(try_from = "String")]
pub struct Base(Url);

impl Base {
    /// Join link with base url
    #[must_use]
    pub(crate) fn join(&self, link: &str) -> Option<Url> {
        // Ensure the link doesn't contain a scheme
        if Url::parse(link).is_ok() {
            return None;
        }
        self.0.join(link).ok()
    }

    pub(crate) fn from_source(source: &InputSource) -> Option<Base> {
        match &source {
            InputSource::RemoteUrl(url) => {
                // Create a new URL with just the scheme, host, and port
                let mut base_url = url.clone();
                base_url.set_path("");
                base_url.set_query(None);
                base_url.set_fragment(None);

                // We keep the username and password intact
                Some(Base(*base_url))
            }
            // other inputs do not have a URL to extract a base
            _ => None,
        }
    }
}

impl TryFrom<&str> for Base {
    type Error = ErrorKind;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let url = Url::parse(value).map_err(|_| {
            ErrorKind::InvalidBase(value.to_string(), "The given URL is invalid".to_string())
        })?;
        if url.cannot_be_a_base() {
            return Err(ErrorKind::InvalidBase(
                value.to_string(),
                "The given URL cannot be a base".to_string(),
            ));
        }
        return Ok(Self(url));
    }
}

impl TryFrom<String> for Base {
    type Error = ErrorKind;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

#[cfg(test)]
mod test_base {
    use crate::Result;

    use super::*;

    #[test]
    fn test_valid_remote() -> Result<()> {
        let base = Base::try_from("https://endler.dev")?;
        assert_eq!(base, Base(Url::parse("https://endler.dev").unwrap()));
        Ok(())
    }

    #[test]
    fn test_invalid_url() {
        assert!(Base::try_from("data:text/plain,Hello?World#").is_err());
    }

    #[test]
    fn test_local_path_string_is_invalid_base() -> Result<()> {
        let cases = vec!["/tmp/lychee", "/tmp/lychee/", "tmp/lychee/"];

        for case in cases {
            assert!(Base::try_from(case).is_err());
        }
        Ok(())
    }

    #[test]
    fn test_get_base_from_url() {
        for (url, expected) in [
            ("https://example.com", "https://example.com"),
            ("https://example.com?query=something", "https://example.com"),
            ("https://example.com/#anchor", "https://example.com"),
            ("https://example.com/foo/bar", "https://example.com"),
            (
                "https://example.com:1234/foo/bar",
                "https://example.com:1234",
            ),
        ] {
            let url = Url::parse(url).unwrap();
            let source = InputSource::RemoteUrl(Box::new(url.clone()));
            let base = Base::from_source(&source);
            let expected = Base(Url::parse(expected).unwrap());
            assert_eq!(base, Some(expected));
        }
    }

    #[test]
    fn test_join_remote() {
        let base = Base(Url::parse("https://example.com").unwrap());
        let url = base.join("foo/bar").unwrap();
        assert_eq!(url, Url::parse("https://example.com/foo/bar").unwrap());
    }

    #[test]
    fn test_join_query_params() {
        let base = Base(Url::parse("https://example.com").unwrap());
        let url = base.join("foo/bar?query=something").unwrap();
        assert_eq!(
            url,
            Url::parse("https://example.com/foo/bar?query=something").unwrap()
        );
    }

    #[test]
    fn test_join_data_invalid() {
        let base = Base(Url::parse("https://example.com").unwrap());
        assert!(base.join("data:text/plain,Hello?World#").is_none());
    }
}
