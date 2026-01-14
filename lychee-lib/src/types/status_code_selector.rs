use std::{collections::HashSet, fmt::Display, hash::BuildHasher, str::FromStr, sync::LazyLock};

use http::StatusCode;
use serde::{Deserialize, de::Visitor};
use thiserror::Error;

use crate::{StatusRangeError, types::accept::StatusRange};

/// These values are the default status codes which are accepted by lychee.
pub static DEFAULT_ACCEPTED_STATUS_CODES: LazyLock<HashSet<StatusCode>> =
    LazyLock::new(|| <HashSet<StatusCode>>::from(StatusCodeSelector::default_accepted()));

#[derive(Debug, Error, PartialEq)]
pub enum StatusCodeSelectorError {
    #[error("invalid/empty input")]
    InvalidInput,

    #[error("failed to parse range: {0}")]
    RangeError(#[from] StatusRangeError),
}

/// A [`StatusCodeSelector`] holds ranges of HTTP status codes, and determines
/// whether a specific code is matched.
#[derive(Clone, Debug, PartialEq)]
pub struct StatusCodeSelector {
    ranges: Vec<StatusRange>,
}

impl FromStr for StatusCodeSelector {
    type Err = StatusCodeSelectorError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();

        if input.is_empty() {
            return Ok(Self::empty());
        }

        let ranges = input
            .split(',')
            .map(|part| StatusRange::from_str(part.trim()))
            .collect::<Result<Vec<StatusRange>, StatusRangeError>>()?;

        Ok(Self::new_from(ranges))
    }
}

impl Display for StatusCodeSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ranges: Vec<_> = self.ranges.iter().map(ToString::to_string).collect();
        write!(f, "{}", ranges.join(","))
    }
}

impl StatusCodeSelector {
    /// Creates a new empty selector
    #[must_use]
    pub const fn empty() -> Self {
        Self { ranges: Vec::new() }
    }

    /// Create a new selector where 100..=103 and 200..300 are selected.
    /// These are the status codes which are accepted by default.
    #[must_use]
    pub fn default_accepted() -> Self {
        #[expect(clippy::missing_panics_doc, reason = "infallible")]
        Self::new_from(vec![
            StatusRange::new(100, 103).unwrap(),
            StatusRange::new(200, 299).unwrap(),
        ])
    }

    /// Creates a new [`StatusCodeSelector`] prefilled with `ranges`.
    #[must_use]
    pub fn new_from(ranges: Vec<StatusRange>) -> Self {
        let mut selector = Self::empty();

        for range in ranges {
            selector.add_range(range);
        }

        selector
    }

    /// Adds a range of HTTP status codes to this [`StatusCodeSelector`].
    /// This method merges the new and existing ranges if they overlap.
    pub fn add_range(&mut self, range: StatusRange) -> &mut Self {
        // Merge with previous range if possible
        if let Some(last) = self.ranges.last_mut()
            && last.merge(&range)
        {
            return self;
        }

        // If neither is the case, the ranges have no overlap at all. Just add
        // to the list of ranges.
        self.ranges.push(range);
        self
    }

    /// Returns whether this [`StatusCodeSelector`] contains `value`.
    #[must_use]
    pub fn contains(&self, value: u16) -> bool {
        self.ranges.iter().any(|range| range.contains(value))
    }

    #[cfg(test)]
    pub(crate) const fn len(&self) -> usize {
        self.ranges.len()
    }
}

impl<S: BuildHasher + Default> From<StatusCodeSelector> for HashSet<StatusCode, S> {
    fn from(value: StatusCodeSelector) -> Self {
        value
            .ranges
            .into_iter()
            .flat_map(<HashSet<StatusCode>>::from)
            .collect()
    }
}

struct StatusCodeSelectorVisitor;

impl<'de> Visitor<'de> for StatusCodeSelectorVisitor {
    type Value = StatusCodeSelector;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string or a sequence of strings")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        StatusCodeSelector::from_str(v).map_err(serde::de::Error::custom)
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let value = u16::try_from(v).map_err(serde::de::Error::custom)?;
        Ok(StatusCodeSelector::new_from(vec![
            StatusRange::new(value, value).map_err(serde::de::Error::custom)?,
        ]))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut selector = StatusCodeSelector::empty();
        while let Some(v) = seq.next_element::<toml::Value>()? {
            if let Some(v) = v.as_integer() {
                let value = u16::try_from(v).map_err(serde::de::Error::custom)?;
                selector
                    .add_range(StatusRange::new(value, value).map_err(serde::de::Error::custom)?);
                continue;
            }

            if let Some(s) = v.as_str() {
                let range = StatusRange::from_str(s).map_err(serde::de::Error::custom)?;
                selector.add_range(range);
                continue;
            }

            return Err(serde::de::Error::custom(
                "failed to parse sequence of accept ranges",
            ));
        }
        Ok(selector)
    }
}

impl<'de> Deserialize<'de> for StatusCodeSelector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StatusCodeSelectorVisitor)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("", vec![], vec![100, 110, 150, 200, 300, 175, 350], 0)]
    #[case("100..=150,200..=300", vec![100, 110, 150, 200, 300], vec![175, 350], 2)]
    #[case("200..=300,100..=250", vec![100, 150, 200, 250, 300], vec![350], 1)]
    #[case("100..=200,150..=200", vec![100, 150, 200], vec![250, 300], 1)]
    #[case("100..=200,300", vec![100, 110, 200, 300], vec![250, 350], 2)]
    fn test_from_str(
        #[case] input: &str,
        #[case] valid_values: Vec<u16>,
        #[case] invalid_values: Vec<u16>,
        #[case] length: usize,
    ) {
        let selector = StatusCodeSelector::from_str(input).unwrap();
        assert_eq!(selector.len(), length);

        for valid in valid_values {
            assert!(selector.contains(valid));
        }

        for invalid in invalid_values {
            assert!(!selector.contains(invalid));
        }
    }

    #[rstest]
    #[case(r"accept = ['200..204', '429']", vec![200, 203, 429], vec![204, 404], 2)]
    #[case(r"accept = '200..204, 429'", vec![200, 203, 429], vec![204, 404], 2)]
    #[case(r"accept = ['200', '429']", vec![200, 429], vec![404], 2)]
    #[case(r"accept = '200, 429'", vec![200, 429], vec![404], 2)]
    #[case(r"accept = [200, 429]", vec![200, 429], vec![404], 2)]
    #[case(r"accept = '200'", vec![200], vec![404], 1)]
    #[case(r"accept = 200", vec![200], vec![404], 1)]
    fn test_deserialize(
        #[case] input: &str,
        #[case] valid_values: Vec<u16>,
        #[case] invalid_values: Vec<u16>,
        #[case] length: usize,
    ) {
        #[derive(Deserialize)]
        struct Config {
            accept: StatusCodeSelector,
        }

        let config: Config = toml::from_str(input).unwrap();
        assert_eq!(config.accept.len(), length);

        for valid in valid_values {
            assert!(config.accept.contains(valid));
        }

        for invalid in invalid_values {
            assert!(!config.accept.contains(invalid));
        }
    }

    #[rstest]
    #[case("100..=150,200..=300", "100..=150,200..=300")]
    #[case("100..=150,300", "100..=150,300..=300")]
    fn test_display(#[case] input: &str, #[case] display: &str) {
        let selector = StatusCodeSelector::from_str(input).unwrap();
        assert_eq!(selector.to_string(), display);
    }

    #[rstest]
    #[case("..=102,200..202,999..", HashSet::from([100, 101, 102, 200, 201,999]))]
    fn test_into_u16_set(#[case] input: &str, #[case] expected: HashSet<u16>) {
        let actual: HashSet<StatusCode> = StatusCodeSelector::from_str(input).unwrap().into();
        let expected = expected
            .into_iter()
            .map(|v| StatusCode::from_u16(v).unwrap())
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_default_accepted_values() {
        // assert that accessing the value does not panic
        let _ = LazyLock::force(&DEFAULT_ACCEPTED_STATUS_CODES);
    }
}
