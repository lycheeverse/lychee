//! Parse and handle custom HTTP headers.
//!
//! Provides utilities for taking user-provided HTTP header strings
//! (e.g. from the CLI or config files) and converting them into strongly
//! typed `reqwest` headers.

use anyhow::{Error, Result, anyhow};
use clap::builder::TypedValueParser;
use http::{
    HeaderMap,
    header::{HeaderName, HeaderValue},
};
use std::{collections::HashMap, str::FromStr};

/// Parse a single header into a [`HeaderName`] and [`HeaderValue`]
///
/// Headers are expected to be in format `Header-Name: Header-Value`.
/// The header name and value are trimmed of whitespace.
///
/// If the header contains multiple colons, the part after the first colon is
/// considered the value.
///
/// # Errors
///
/// This fails if the header does not contain exactly one `:` character or
/// if the header name contains non-ASCII characters.
pub(crate) fn parse_single_header(header: &str) -> Result<(HeaderName, HeaderValue)> {
    let parts: Vec<&str> = header.splitn(2, ':').collect();
    match parts.as_slice() {
        [name, value] => {
            let name = name.trim();
            let name = HeaderName::from_str(name)
                .map_err(|e| anyhow!("Unable to convert header name '{name}': {e}"))?;
            let value = HeaderValue::from_str(value.trim())
                .map_err(|e| anyhow!("Unable to read value of header with name '{name}': {e}"))?;
            Ok((name, value))
        }
        _ => Err(anyhow!(
            "Invalid header format. Expected colon-separated string in the format 'HeaderName: HeaderValue'"
        )),
    }
}

/// Parses a single HTTP header into a tuple of (String, String)
///
/// This does NOT merge multiple headers into one.
#[derive(Clone, Debug)]
pub(crate) struct HeaderParser;

impl TypedValueParser for HeaderParser {
    type Value = (String, String);

    fn parse_ref(
        &self,
        _cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let header_str = value.to_str().ok_or_else(|| {
            clap::Error::raw(
                clap::error::ErrorKind::InvalidValue,
                "Header value contains invalid UTF-8",
            )
        })?;

        match parse_single_header(header_str) {
            Ok((name, value)) => {
                let Ok(value) = value.to_str() else {
                    return Err(clap::Error::raw(
                        clap::error::ErrorKind::InvalidValue,
                        "Header value contains invalid UTF-8",
                    ));
                };

                Ok((name.to_string(), value.to_string()))
            }
            Err(e) => Err(clap::Error::raw(
                clap::error::ErrorKind::InvalidValue,
                e.to_string(),
            )),
        }
    }
}

impl clap::builder::ValueParserFactory for HeaderParser {
    type Parser = HeaderParser;
    fn value_parser() -> Self::Parser {
        HeaderParser
    }
}

/// Extension trait for converting a map of header pairs to a `HeaderMap`
pub(crate) trait HeaderMapExt {
    /// Convert a collection of header key-value pairs to a `HeaderMap`
    ///
    /// # Errors
    ///
    /// This fails if any header name or value cannot be parsed into a valid
    /// `HeaderName` or `HeaderValue` respectively.
    fn from_header_pairs(headers: &HashMap<String, String>) -> Result<HeaderMap, Error>;
}

impl HeaderMapExt for HeaderMap {
    fn from_header_pairs(headers: &HashMap<String, String>) -> Result<HeaderMap, Error> {
        let mut header_map = HeaderMap::new();
        for (name, value) in headers {
            let header_name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|e| anyhow!("Invalid header name '{name}': {e}"))?;
            let header_value = HeaderValue::from_str(value)
                .map_err(|e| anyhow!("Invalid header value '{value}': {e}"))?;
            header_map.insert(header_name, header_value);
        }
        Ok(header_map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_custom_headers() {
        assert_eq!(
            parse_single_header("accept:text/html").unwrap(),
            (
                HeaderName::from_static("accept"),
                HeaderValue::from_static("text/html")
            )
        );
    }

    #[test]
    fn test_parse_custom_header_multiple_colons() {
        assert_eq!(
            parse_single_header("key:x-test:check=this").unwrap(),
            (
                HeaderName::from_static("key"),
                HeaderValue::from_static("x-test:check=this")
            )
        );
    }

    #[test]
    fn test_parse_custom_headers_with_equals() {
        assert_eq!(
            parse_single_header("key:x-test=check=this").unwrap(),
            (
                HeaderName::from_static("key"),
                HeaderValue::from_static("x-test=check=this")
            )
        );
    }

    #[test]
    /// We should not reveal potentially sensitive data contained in the headers.
    /// See: [#1297](https://github.com/lycheeverse/lychee/issues/1297)
    fn test_does_not_echo_sensitive_data() {
        let error = parse_single_header("My-Header💣: secret")
            .expect_err("Should not allow unicode as key");
        assert!(!error.to_string().contains("secret"));

        let error = parse_single_header("secret").expect_err("Should fail when no `:` given");
        assert!(!error.to_string().contains("secret"));
    }
}
