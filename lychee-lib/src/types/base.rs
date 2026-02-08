use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, path::PathBuf};

use crate::{ErrorKind, ResolvedInputSource};

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
            Self::Local(path) => {
                let full_path = path.join(link);
                Url::from_file_path(full_path).ok()
            }
        }
    }

    pub(crate) fn from_source(source: &ResolvedInputSource) -> Option<Base> {
        match &source {
            ResolvedInputSource::RemoteUrl(url) => {
                // Create a new URL with just the scheme, host, and port
                let mut base_url = url.clone();
                base_url.set_path("");
                base_url.set_query(None);
                base_url.set_fragment(None);

                // We keep the username and password intact
                Some(Base::Remote(*base_url))
            }
            // other inputs do not have a URL to extract a base
            _ => None,
        }
    }
}

impl TryFrom<&str> for Base {
    type Error = ErrorKind;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        // Check if the value is an absolute path first.
        //
        // This is necessary because on Windows, absolute paths (e.g. `C:\path`)
        // are parsed as URLs with a single-letter scheme (`C`) and a path
        // (`\path`). These are treated as opaque URLs, which cannot be used as
        // a base for joining relative links.
        //
        // In the context of URLs, "opaque" means the URL does not follow a
        // hierarchical structure (like `/folder/file.txt`). Instead, it is just
        // a scheme followed by a blob of data.
        //
        // Common examples of opaque URLs are `mailto:user@example.com` or
        // `data:text/plain,...`. We cannot use these as a base to resolve
        // relative paths (e.g., you can't join `../image.png` to a `mailto`
        // link).
        //
        // The issue on Windows is that an absolute path like `C:\foo\bar` is
        // technically a valid URL (scheme: `C`, data: `\foo\bar`).
        //
        // Because `C` isn't a "special" scheme (like `http` or `file`) and
        // there are no double slashes `//`, the URL parser treats it as an
        // opaque URL. Since opaque URLs cannot be used as a base, `lychee`
        // would reject these valid Windows paths if we didn't explicitly check
        // for them as files first.
        let path = PathBuf::from(value);
        if path.is_absolute() {
            return Ok(Self::Local(path));
        }

        if let Ok(url) = Url::parse(value) {
            if url.cannot_be_a_base() {
                return Err(ErrorKind::InvalidBase(
                    value.to_string(),
                    "The given URL cannot be used as a base URL".to_string(),
                ));
            }
            return Ok(Self::Remote(url));
        }

        Err(ErrorKind::InvalidBase(
            value.to_string(),
            "Base must either be a URL (with scheme) or an absolute local path".to_string(),
        ))
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
    fn test_valid_local_path_string_as_base() -> Result<()> {
        #[cfg(not(windows))]
        let cases = vec!["/tmp/lychee", "/tmp/lychee/"];

        #[cfg(windows)]
        let cases = vec![r"C:\tmp\lychee", r"C:\tmp\lychee\"];

        for case in cases {
            assert_eq!(Base::try_from(case)?, Base::Local(PathBuf::from(case)));
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
            let source = ResolvedInputSource::RemoteUrl(Box::new(url.clone()));
            let base = Base::from_source(&source);
            let expected = Base::Remote(Url::parse(expected).unwrap());
            assert_eq!(base, Some(expected));
        }
    }
}
