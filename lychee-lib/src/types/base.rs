use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, path::PathBuf};

use crate::{ErrorKind, InputSource};

/// When encountering links without a full domain in a document,
/// the base determines where this resource can be found.
/// Both, local and remote targets are supported.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
#[allow(variant_size_differences)]
#[serde(try_from = "String")]
pub enum Base {
    /// Local file path pointing to root directory
    Local(PathBuf),
    /// Remote URL pointing to a website homepage
    Remote(Url),
}

impl Base {
    /// Join link with base url
    #[must_use]
    pub(crate) fn join(&self, link: &str) -> Option<Url> {
        match self {
            Self::Remote(url) => url.join(link).ok(),
            Self::Local(_) => None,
        }
    }

    /// Return the directory if the base is local
    #[must_use]
    pub(crate) fn dir(&self) -> Option<PathBuf> {
        match self {
            Self::Remote(_) => None,
            Self::Local(d) => Some(d.clone()),
        }
    }

    pub(crate) fn from_source(source: &InputSource) -> Option<Url> {
        match &source {
            InputSource::RemoteUrl(url) => {
                // TODO: This should be refactored.
                // Cases like https://user:pass@example.com are not handled
                // We can probably use the original URL and just replace the
                // path component in the caller of this function
                if let Some(port) = url.port() {
                    Url::parse(&format!("{}://{}:{port}", url.scheme(), url.host_str()?)).ok()
                } else {
                    Url::parse(&format!("{}://{}", url.scheme(), url.host_str()?)).ok()
                }
            }
            // other inputs do not have a URL to extract a base
            _ => None,
        }
    }
}

impl TryFrom<&str> for Base {
    type Error = ErrorKind;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if let Ok(url) = Url::parse(value) {
            if url.cannot_be_a_base() {
                return Err(ErrorKind::InvalidBase(
                    value.to_string(),
                    "The given URL cannot be a base".to_string(),
                ));
            }
            return Ok(Self::Remote(url));
        }
        Ok(Self::Local(PathBuf::from(value)))
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
        assert_eq!(
            base,
            Base::Remote(Url::parse("https://endler.dev").unwrap())
        );
        Ok(())
    }

    #[test]
    fn test_invalid_url() {
        assert!(Base::try_from("data:text/plain,Hello?World#").is_err());
    }

    #[test]
    fn test_valid_local() -> Result<()> {
        let dir = tempfile::tempdir().unwrap();
        Base::try_from(dir.as_ref().to_str().unwrap())?;
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
            let expected = Url::parse(expected).unwrap();
            assert_eq!(base, Some(expected));
        }
    }
}
