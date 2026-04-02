use std::str::FromStr;

use serde_with::DeserializeFromStr;
use thiserror::Error;

use crate::{BasicAuthCredentials, types::basic_auth::BasicAuthCredentialsParseError};

#[derive(Clone, Debug, Error, PartialEq)]
pub enum BasicAuthSelectorParseError {
    #[error("Empty selector input")]
    EmptyInput,

    #[error("Missing basic auth credentials or URI. Valid form is '<uri> <username>:<password>'")]
    InvalidSyntax,

    #[error(
        "Too many space separated values. Expected 2, got {0}. Valid form is '<uri> <username>:<password>'"
    )]
    TooManyParts(usize),

    #[error("Basic auth credentials error")]
    BasicAuthCredentialsParseError(#[from] BasicAuthCredentialsParseError),

    #[error("Regex compile error")]
    RegexError(#[from] regex::Error),
}

/// [`BasicAuthSelector`] provides basic auth credentials for URLs which match
/// the specified regex. This allows users to set different credentials based
/// on the URLs they want to target.
#[derive(Debug, Clone, DeserializeFromStr, PartialEq)]
pub struct BasicAuthSelector {
    /// The basic auth credentials made up of username and password
    pub credentials: BasicAuthCredentials,

    /// This regex matches URLs which will use the basic auth credentials
    pub raw_uri_regex: String,
}

impl FromStr for BasicAuthSelector {
    type Err = BasicAuthSelectorParseError;

    fn from_str(selector: &str) -> Result<Self, Self::Err> {
        let selector = selector.trim();

        if selector.is_empty() {
            return Err(BasicAuthSelectorParseError::EmptyInput);
        }

        let parts: Vec<_> = selector.split(' ').collect();

        if parts.len() <= 1 {
            return Err(BasicAuthSelectorParseError::InvalidSyntax);
        }

        if parts.len() > 2 {
            return Err(BasicAuthSelectorParseError::TooManyParts(parts.len()));
        }

        Ok(Self {
            credentials: parts[1].parse()?,
            raw_uri_regex: parts[0].to_string(),
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_valid_basic_auth_selector() {
        let input = "http://example.com foo:bar";
        let selector: BasicAuthSelector = input.parse().unwrap();

        assert_eq!(selector.raw_uri_regex, "http://example.com".to_string());
        assert_eq!(
            selector.credentials,
            BasicAuthCredentials {
                username: "foo".to_string(),
                password: "bar".to_string()
            }
        );
    }

    #[test]
    fn test_missing_uri_basic_auth_selector() {
        let input = "foo:bar";
        let result = BasicAuthSelector::from_str(input);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            BasicAuthSelectorParseError::InvalidSyntax
        );
    }

    #[test]
    fn test_missing_credentials_basic_auth_selector() {
        let input = "https://example.com";
        let result = BasicAuthSelector::from_str(input);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            BasicAuthSelectorParseError::InvalidSyntax
        );
    }

    #[test]
    fn test_empty_basic_auth_selector() {
        let input = "";
        let result = BasicAuthSelector::from_str(input);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), BasicAuthSelectorParseError::EmptyInput);

        let input = "   ";
        let result = BasicAuthSelector::from_str(input);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), BasicAuthSelectorParseError::EmptyInput);
    }

    #[test]
    fn test_too_many_parts_basic_auth_selector() {
        let input = "";
        let result = BasicAuthSelector::from_str(input);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), BasicAuthSelectorParseError::EmptyInput);

        let input = "   ";
        let result = BasicAuthSelector::from_str(input);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), BasicAuthSelectorParseError::EmptyInput);
    }
}
