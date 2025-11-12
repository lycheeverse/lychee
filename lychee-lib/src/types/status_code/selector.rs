use std::{collections::HashSet, fmt::Display, hash::BuildHasher, str::FromStr, sync::LazyLock};

use http::StatusCode;
use serde::{Deserialize, de::Visitor};
use thiserror::Error;

use crate::{AcceptRangeError, types::accept::AcceptRange};

/// These values are the default status codes which are accepted by lychee.
/// SAFETY: This does not panic as all provided status codes are valid.
pub static DEFAULT_ACCEPTED_STATUS_CODES: LazyLock<HashSet<StatusCode>> =
    LazyLock::new(|| <HashSet<StatusCode>>::try_from(StatusCodeSelector::default()).unwrap());

#[derive(Debug, Error, PartialEq)]
pub enum StatusCodeSelectorError {
    #[error("invalid/empty input")]
    InvalidInput,

    #[error("failed to parse accept range: {0}")]
    AcceptRangeError(#[from] AcceptRangeError),
}

/// A [`StatusCodeSelector`] holds ranges of HTTP status codes, and determines
/// whether a specific code is matched, so the link can be counted as valid (not
/// broken) or excluded. `StatusCodeSelector` differs from
/// [`StatusCodeExcluder`](super::excluder::StatusCodeExcluder)
///  in the defaults it provides. As this is meant to
/// select valid status codes, the default includes every successful status.
#[derive(Clone, Debug, PartialEq)]
pub struct StatusCodeSelector {
    ranges: Vec<AcceptRange>,
}

impl FromStr for StatusCodeSelector {
    type Err = StatusCodeSelectorError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();

        if input.is_empty() {
            return Err(StatusCodeSelectorError::InvalidInput);
        }

        let ranges = input
            .split(',')
            .map(|part| AcceptRange::from_str(part.trim()))
            .collect::<Result<Vec<AcceptRange>, AcceptRangeError>>()?;

        Ok(Self::new_from(ranges))
    }
}

/// These values are the default status codes which are accepted by lychee.
impl Default for StatusCodeSelector {
    fn default() -> Self {
        Self::new_from(vec![AcceptRange::new(100, 103), AcceptRange::new(200, 299)])
    }
}

impl Display for StatusCodeSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ranges: Vec<_> = self.ranges.iter().map(ToString::to_string).collect();
        write!(f, "{}", ranges.join(","))
    }
}

impl StatusCodeSelector {
    /// Creates a new empty [`StatusCodeSelector`].
    #[must_use]
    pub const fn new() -> Self {
        Self { ranges: Vec::new() }
    }

    /// Creates a new [`StatusCodeSelector`] prefilled with `ranges`.
    #[must_use]
    pub fn new_from(ranges: Vec<AcceptRange>) -> Self {
        let mut selector = Self::new();

        for range in ranges {
            selector.add_range(range);
        }

        selector
    }

    /// Adds a range of HTTP status codes to this [`StatusCodeSelector`].
    /// This method merges the new and existing ranges if they overlap.
    pub fn add_range(&mut self, range: AcceptRange) -> &mut Self {
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

impl<S: BuildHasher + Default> From<StatusCodeSelector> for HashSet<u16, S> {
    fn from(value: StatusCodeSelector) -> Self {
        value
            .ranges
            .into_iter()
            .flat_map(|range| range.inner().collect::<Vec<_>>())
            .collect()
    }
}

impl<S: BuildHasher + Default> TryFrom<StatusCodeSelector> for HashSet<StatusCode, S> {
    type Error = http::status::InvalidStatusCode;

    fn try_from(value: StatusCodeSelector) -> Result<Self, Self::Error> {
        <HashSet<u16>>::from(value)
            .into_iter()
            .map(StatusCode::from_u16)
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
        Ok(StatusCodeSelector::new_from(vec![AcceptRange::new(
            value, value,
        )]))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut selector = StatusCodeSelector::new();
        while let Some(v) = seq.next_element::<toml::Value>()? {
            if let Some(v) = v.as_integer() {
                let value = u16::try_from(v).map_err(serde::de::Error::custom)?;
                selector.add_range(AcceptRange::new(value, value));
                continue;
            }

            if let Some(s) = v.as_str() {
                let range = AcceptRange::from_str(s).map_err(serde::de::Error::custom)?;
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
    #[case("100..=102,200..202", HashSet::from([100, 101, 102, 200, 201]))]
    fn test_into_u16_set(#[case] input: &str, #[case] expected: HashSet<u16>) {
        let actual: HashSet<u16> = StatusCodeSelector::from_str(input).unwrap().into();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_default_accepted_values() {
        // assert that accessing the value does not panic as described in the SAFETY note.
        let _ = &*DEFAULT_ACCEPTED_STATUS_CODES;
    }
}
