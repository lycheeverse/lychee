use percent_encoding::percent_decode_str;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, path::Path, path::PathBuf};

use crate::utils;
use crate::{ErrorKind, ResolvedInputSource, Result};

/// When encountering links without a full domain in a document,
/// the base determines where this resource can be found.
/// Both, local and remote targets are supported. Local paths
/// are represented as file:// URLs.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
#[allow(variant_size_differences)]
#[serde(try_from = "String")]
pub struct Base {
    /// Base URL. This **must** be a valid base. That is, [`Url::cannot_be_a_base`] must be false.
    base_url: Url,
}

impl Base {
    /// Parses the given link text using the current base URL. The given link may be an absolute or
    /// relative link. This may fail if the link text is not a valid absolute or relative URL.
    ///
    /// If the base is a file:// URL and the link is a root-relative link, then the path of the
    /// base URL will be prefixed onto the link. That is, any root-relative link will become a
    /// subpath of the file:// base URL.
    #[must_use]
    pub(crate) fn join(&self, link: &str) -> Option<Url> {
        let link = link.trim_start();
        let base_url = &self.base_url;
        let mut url = base_url.join(link).ok()?;

        match (base_url.scheme(), url.scheme()) {
            ("file", "file") if link.starts_with('/') && !link.starts_with("//") => {
                let url_parts: Vec<String> = url
                    .path_segments()
                    .expect("must be a base")
                    .map(std::string::ToString::to_string)
                    .collect();

                url.path_segments_mut()
                    .expect("must be a base")
                    .clear()
                    .extend(
                        base_url
                            .path_segments()
                            .expect("must be a base")
                            .map(|x| percent_decode_str(x).decode_utf8_lossy()),
                    )
                    .pop_if_empty()
                    .extend(
                        url_parts
                            .iter()
                            .map(|x| percent_decode_str(x).decode_utf8_lossy()),
                    );
            }
            _ => (),
        }

        Some(url)
    }

    /// Constructs a [`Base`] from the given URL, requiring that the given path be acceptable as a
    /// base URL. That is, it cannot be a special scheme like `data:`.
    ///
    /// # Errors
    ///
    /// Errors if the given URL cannot be a base.
    pub fn from_url(url: Url) -> Result<Base> {
        if url.cannot_be_a_base() {
            return Err(ErrorKind::InvalidBase(
                url.to_string(),
                "The given URL cannot be used as a base URL".to_string(),
            ));
        }

        Ok(Self { base_url: url })
    }

    /// Constructs a [`Base`] from the given filesystem path, requiring that the given path be
    /// absolute.
    ///
    /// # Errors
    ///
    /// Errors if the given path is not an absolute path.
    pub fn from_path(path: &Path) -> Result<Base> {
        let Ok(url) = Url::from_directory_path(path) else {
            return Err(ErrorKind::InvalidBase(
                path.to_string_lossy().to_string(),
                "Base must either be a full URL (with scheme) or an absolute local path"
                    .to_string(),
            ));
        };

        Self::from_url(url)
    }

    pub(crate) fn from_source(source: &ResolvedInputSource) -> Option<Base> {
        match &source {
            ResolvedInputSource::RemoteUrl(url) => {
                // Create a new URL with just the scheme, host, and port
                let mut base_url = *url.clone();
                base_url.set_path("");
                base_url.set_query(None);
                base_url.set_fragment(None);

                // We keep the username and password intact
                Self::from_url(base_url).ok()
            }
            // other inputs do not have a URL to extract a base
            _ => None,
        }
    }

    pub(crate) fn to_path(&self) -> Option<PathBuf> {
        if self.base_url.scheme() == "file" {
            self.base_url.to_file_path().ok()
        } else {
            None
        }
    }
}

impl TryFrom<&str> for Base {
    type Error = ErrorKind;

    fn try_from(value: &str) -> Result<Self> {
        match utils::url::parse_url_or_path(value) {
            Ok(url) => Base::from_url(url),
            Err(path) => Base::from_path(&PathBuf::from(path)),
        }
    }
}

impl TryFrom<String> for Base {
    type Error = ErrorKind;

    fn try_from(value: String) -> Result<Self> {
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
            Base::from_url(Url::parse("https://endler.dev").unwrap())?
        );
        Ok(())
    }

    #[test]
    fn test_invalid_url() {
        assert!(Base::try_from("data:text/plain,Hello?World#").is_err());
    }

    #[test]
    fn test_valid_local_path_string_as_base() -> Result<()> {
        let cases = vec!["/tmp/lychee", "/tmp/lychee/"];

        for case in cases {
            assert_eq!(
                Base::try_from(case)?,
                Base::from_path(&PathBuf::from(case)).unwrap()
            );
        }
        Ok(())
    }

    #[test]
    fn test_invalid_local_path_string_as_base() {
        let cases = vec!["a", "tmp/lychee/", "example.com", "../nonlocal"];

        for case in cases {
            assert!(Base::try_from(case).is_err());
        }
    }

    #[test]
    fn test_valid_local() -> Result<()> {
        let dir = tempfile::tempdir().unwrap();
        Base::try_from(dir.as_ref().to_str().unwrap())?;
        Ok(())
    }

    #[test]
    fn test_get_base_from_url() -> Result<()> {
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
            let source = ResolvedInputSource::RemoteUrl(Box::new(url.clone()));
            let base = Base::from_source(&source);
            let expected = Base::from_url(Url::parse(expected).unwrap())?;
            assert_eq!(base, Some(expected));
        }

        Ok(())
    }
}
