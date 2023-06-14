use std::str::FromStr;

use headers::{authorization::Basic, Authorization};
use regex::Regex;
use serde::Deserialize;
use serde_with::DeserializeFromStr;
use thiserror::Error;

#[derive(Copy, Clone, Debug, Error)]
pub enum BasicAuthCredentialsParseError {
    #[error("Invalid basic auth credentials syntax")]
    InvalidSyntax,

    #[error("Missing basic auth password")]
    MissingPassword,

    #[error("Missing basic auth username")]
    MissingUsername,

    #[error("Too many values separated by colon. Expected 2, got {0}. Valid form is '<username>:<password>'")]
    TooManyParts(usize),
}

/// [`BasicAuthCredentials`] contains a pair of basic auth values consisting of
/// a username and password.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BasicAuthCredentials {
    /// Basic auth username
    pub username: String,

    /// Basic auth password
    pub password: String,
}

impl FromStr for BasicAuthCredentials {
    type Err = BasicAuthCredentialsParseError;

    fn from_str(credentials: &str) -> Result<Self, Self::Err> {
        let parts: Vec<_> = credentials.trim().split(':').collect();

        if parts.len() == 1 {
            return Err(BasicAuthCredentialsParseError::InvalidSyntax);
        }

        if parts.len() != 2 {
            return Err(BasicAuthCredentialsParseError::TooManyParts(parts.len()));
        }

        if parts[0].is_empty() {
            return Err(BasicAuthCredentialsParseError::MissingUsername);
        }

        if parts[1].is_empty() {
            return Err(BasicAuthCredentialsParseError::MissingPassword);
        }

        Ok(Self {
            username: parts[0].to_string(),
            password: parts[1].to_string(),
        })
    }
}

#[derive(Clone, Debug, Error)]
pub enum BasicAuthSelectorParseError {
    #[error("Missing basic auth credentials, only provided URL. Valid form is '<url> <username>:<password>'")]
    MissingCredentials,

    #[error("Too many space separated values. Expected 2, got {0}. Valid form is '<url> <username>:<password>'")]
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
pub struct RawBasicAuthSelector {
    /// The basic auth credentials made up of username and password
    pub credentials: BasicAuthCredentials,

    /// This regex matches URLs which will use the basic auth credentials
    pub url: String,
}

impl FromStr for RawBasicAuthSelector {
    type Err = BasicAuthSelectorParseError;

    fn from_str(selector: &str) -> Result<Self, Self::Err> {
        let parts: Vec<_> = selector.trim().split(' ').collect();

        if parts.len() == 1 {
            return Err(BasicAuthSelectorParseError::MissingCredentials);
        }

        if parts.len() != 2 {
            return Err(BasicAuthSelectorParseError::TooManyParts(parts.len()));
        }

        Ok(Self {
            credentials: parts[1].parse()?,
            url: parts[0].to_string(),
        })
    }
}

/// [`BasicAuthSelector`] provides basic auth credentials for URLs which match
/// the specified regex. This allows users to set different credentials based
/// on the URLs they want to target. The basic auth username and password will
/// be used by the URL matched by the compiled regex.
#[derive(Clone, Debug)]
pub struct BasicAuthSelector {
    /// The basic auth credentials made up of username and password
    pub credentials: Authorization<Basic>,

    /// This regex matches URLs which will use the basic auth credentials
    pub url: Regex,
}

impl FromStr for BasicAuthSelector {
    type Err = BasicAuthSelectorParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let selector: RawBasicAuthSelector = input.parse()?;
        BasicAuthSelector::try_from(selector)
    }
}

impl TryFrom<RawBasicAuthSelector> for BasicAuthSelector {
    type Error = BasicAuthSelectorParseError;

    fn try_from(selector: RawBasicAuthSelector) -> Result<Self, Self::Error> {
        Self::try_from(&selector)
    }
}

impl TryFrom<&RawBasicAuthSelector> for BasicAuthSelector {
    type Error = BasicAuthSelectorParseError;

    fn try_from(selector: &RawBasicAuthSelector) -> Result<Self, Self::Error> {
        Ok(Self {
            credentials: Authorization::basic(
                &selector.credentials.username,
                &selector.credentials.password,
            ),
            url: Regex::new(&selector.url)?,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_valid_basic_auth_selector() {
        let input = "http://example.com foo:bar";
        let selector: RawBasicAuthSelector = input.parse().unwrap();

        assert_eq!(selector.url, "http://example.com".to_string());
        assert_eq!(
            selector.credentials,
            BasicAuthCredentials {
                username: "foo".to_string(),
                password: "bar".to_string()
            }
        );
    }
}
