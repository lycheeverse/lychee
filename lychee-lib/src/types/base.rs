use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, path::PathBuf};

use crate::ErrorKind;

/// When encountering links without a full domain in a document,
/// the base determines where this resource can be found.
/// Both, local and remote targets are supported.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
#[allow(variant_size_differences)]
pub enum Base {
    /// Local file path pointing to root directory
    Local(PathBuf),
    /// Remote URL pointing to a website homepage
    Remote(Url),
}

impl Base {
    /// Join link with base url
    pub fn join(&self, link: &str) -> Option<Url> {
        match self {
            Self::Remote(url) => url.join(link).ok(),
            Self::Local(_) => None,
        }
    }

    /// Return the directory if the base is local
    pub fn dir(&self) -> Option<PathBuf> {
        match self {
            Self::Remote(_) => None,
            Self::Local(d) => Some(d.to_path_buf()),
        }
    }
}

impl TryFrom<&str> for Base {
    type Error = ErrorKind;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if let Ok(url) = Url::parse(&value) {
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
        let dir = tempfile::tempdir()?;
        Base::try_from(dir.as_ref().to_str().unwrap())?;
        Ok(())
    }
}
