//! HTTP methods type
//!
//! This module defines the [`Methods`] type, which represents an ordered,
//! non-empty list of HTTP request methods. lychee accepts multiple methods for
//! a single request, trying them in order and returning the first success.
//!
//! Servers, which block certain HTTP methods (e.g. `HEAD`), can be configured
//! to fall back to other methods (e.g. `GET`) for checking.

use std::{fmt::Display, str::FromStr};

use http::Method;
use serde::{Deserialize, de::Visitor};
use thiserror::Error;

/// Error returned when parsing or constructing [`Methods`] fails.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MethodsError {
    /// No method was provided, but at least one is required.
    #[error("at least one HTTP method must be specified")]
    Empty,

    /// The given string is not a valid HTTP method.
    #[error("invalid HTTP method: {0}")]
    InvalidMethod(String),
}

/// An ordered, non-empty list of HTTP request methods.
///
/// The list is guaranteed to be non-empty by construction. A `Methods` can only
/// be created from a single [`Method`] or by constructing it from a collection
/// via a fallible conversion.
//
// TODO: Non-emptiness is currently only enforced at runtime via the fallible
// conversions. We could consider enforcing it at compile time, e.g. by a crate
// like `nonempty` or `vec1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Methods(Vec<Method>);

impl Methods {
    /// Returns the methods in order, starting with the primary method.
    pub fn iter(&self) -> impl Iterator<Item = &Method> {
        self.0.iter()
    }

    /// Returns the first (primary) method.
    #[must_use]
    pub fn first(&self) -> &Method {
        self.0.first().expect(
            "Methods is guaranteed to be non-empty by construction. This is a bug in lychee.",
        )
    }
}

impl From<Method> for Methods {
    fn from(method: Method) -> Self {
        Self(vec![method])
    }
}

impl TryFrom<Vec<Method>> for Methods {
    type Error = MethodsError;

    fn try_from(methods: Vec<Method>) -> Result<Self, Self::Error> {
        if methods.is_empty() {
            return Err(MethodsError::Empty);
        }
        Ok(Self(methods))
    }
}

impl FromStr for Methods {
    type Err = MethodsError;

    /// Parses a comma-separated list of HTTP methods, e.g. `"head,get"`.
    ///
    /// This gets used to parse CLI arguments.
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let methods = input
            .split(',')
            .map(str::trim)
            .map(|token| {
                if token.is_empty() {
                    return Err(MethodsError::Empty);
                }
                parse_method(token)
            })
            .collect::<Result<Vec<Method>, MethodsError>>()?;

        Self::try_from(methods)
    }
}

impl Display for Methods {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let methods: Vec<_> = self.0.iter().map(Method::as_str).collect();
        write!(f, "{}", methods.join(","))
    }
}

/// Parses a single HTTP method
///
/// This is case-insensitive, so e.g. "get" and "GET" are both accepted.
fn parse_method(token: &str) -> Result<Method, MethodsError> {
    Method::from_str(&token.to_uppercase())
        .map_err(|_| MethodsError::InvalidMethod(token.to_string()))
}

struct MethodsVisitor;

impl<'de> Visitor<'de> for MethodsVisitor {
    type Value = Methods;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("an HTTP method string or a sequence of HTTP method strings")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Methods::from_str(v).map_err(serde::de::Error::custom)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut methods = Vec::new();
        while let Some(s) = seq.next_element::<String>()? {
            methods.push(parse_method(&s).map_err(serde::de::Error::custom)?);
        }
        Methods::try_from(methods).map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Methods {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(MethodsVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("get", vec![Method::GET])]
    #[case("HEAD", vec![Method::HEAD])]
    #[case("head,get", vec![Method::HEAD, Method::GET])]
    #[case("head, get", vec![Method::HEAD, Method::GET])]
    #[case(" head , get ", vec![Method::HEAD, Method::GET])]
    fn test_from_str(#[case] input: &str, #[case] expected: Vec<Method>) {
        let methods = Methods::from_str(input).unwrap();
        assert_eq!(methods.iter().cloned().collect::<Vec<_>>(), expected);
    }

    #[test]
    fn test_from_str_empty_is_error() {
        assert_eq!(Methods::from_str(""), Err(MethodsError::Empty));
        assert_eq!(Methods::from_str("  ,  "), Err(MethodsError::Empty));
    }

    #[test]
    fn test_from_str_invalid_method() {
        assert_eq!(
            Methods::from_str("ge t"),
            Err(MethodsError::InvalidMethod("ge t".to_string()))
        );
    }

    #[test]
    fn test_try_from_empty_vec_is_error() {
        assert_eq!(Methods::try_from(Vec::new()), Err(MethodsError::Empty));
    }

    #[test]
    fn test_from_single_method() {
        let methods = Methods::from(Method::HEAD);
        assert_eq!(methods.first(), &Method::HEAD);
        assert_eq!(methods.iter().count(), 1);
    }

    #[rstest]
    #[case(r#"method = "get""#, vec![Method::GET])]
    #[case(r#"method = "head,get""#, vec![Method::HEAD, Method::GET])]
    #[case(r#"method = ["head", "get"]"#, vec![Method::HEAD, Method::GET])]
    #[case(r#"method = ["HEAD", "GET"]"#, vec![Method::HEAD, Method::GET])]
    fn test_deserialize(#[case] input: &str, #[case] expected: Vec<Method>) {
        #[derive(Deserialize)]
        struct Config {
            method: Methods,
        }

        let config: Config = toml::from_str(input).unwrap();
        assert_eq!(config.method.iter().cloned().collect::<Vec<_>>(), expected);
    }

    #[test]
    fn test_deserialize_empty_seq_is_error() {
        #[derive(Deserialize)]
        struct Config {
            #[allow(dead_code)]
            method: Methods,
        }

        assert!(toml::from_str::<Config>(r"method = []").is_err());
    }

    #[test]
    fn test_display() {
        let methods = Methods::from_str("head,get").unwrap();
        assert_eq!(methods.to_string(), "HEAD,GET");
    }
}
