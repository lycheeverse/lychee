use std::{collections::HashSet, fmt::Display, str::FromStr};

use serde::{de::Visitor, Deserialize};
use thiserror::Error;

use crate::{types::accept::AcceptRange, AcceptRangeError};

#[derive(Debug, Error)]
pub enum AcceptSelectorError {
    #[error("invalid/empty input")]
    InvalidInput,

    #[error("failed to parse accept range")]
    AcceptRangeError(#[from] AcceptRangeError),
}

/// An [`AcceptSelector`] determines if a returned HTTP status code should be
/// accepted and thus counted as a valid (not broken) link.
#[derive(Clone, Debug, PartialEq)]
pub struct AcceptSelector {
    ranges: Vec<AcceptRange>,
}

impl FromStr for AcceptSelector {
    type Err = AcceptSelectorError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();

        if input.is_empty() {
            return Err(AcceptSelectorError::InvalidInput);
        }

        let ranges = input
            .split(',')
            .map(|part| AcceptRange::from_str(part.trim()))
            .collect::<Result<Vec<AcceptRange>, AcceptRangeError>>()?;

        Ok(Self::new_from(ranges))
    }
}

impl Default for AcceptSelector {
    fn default() -> Self {
        Self::new_from(vec![
            AcceptRange::new(100, 103),
            AcceptRange::new(200, 299),
            AcceptRange::new(403, 403),
        ])
    }
}

impl Display for AcceptSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ranges: Vec<_> = self.ranges.iter().map(|r| r.to_string()).collect();
        write!(f, "[{}]", ranges.join(","))
    }
}

impl AcceptSelector {
    /// Creates a new empty [`AcceptSelector`].
    #[must_use]
    pub fn new() -> Self {
        Self { ranges: Vec::new() }
    }

    /// Creates a new [`AcceptSelector`] prefilled with `ranges`.
    #[must_use]
    pub fn new_from(ranges: Vec<AcceptRange>) -> Self {
        let mut selector = Self::new();

        for range in ranges {
            selector.add_range(range);
        }

        selector
    }

    /// Adds a range of accepted HTTP status codes to this [`AcceptSelector`].
    /// This method merges the new and existing ranges if they overlap.
    pub fn add_range(&mut self, range: AcceptRange) -> &mut Self {
        // Merge with previous range if possible
        if let Some(last) = self.ranges.last_mut() {
            if last.merge(&range) {
                return self;
            }
        }

        // If neither is the case, the ranges have no overlap at all. Just add
        // to the list of ranges.
        self.ranges.push(range);
        self
    }

    /// Returns whether this [`AcceptSelector`] contains `value`.
    #[must_use]
    pub fn contains(&self, value: u16) -> bool {
        self.ranges.iter().any(|range| range.contains(value))
    }

    /// Consumes self and creates a [`HashSet`] which contains all
    /// accepted status codes.
    #[must_use]
    pub fn into_set(self) -> HashSet<u16> {
        let mut set = HashSet::new();

        for range in self.ranges {
            for value in range.inner() {
                set.insert(value);
            }
        }

        set
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.ranges.len()
    }
}

struct AcceptSelectorVisitor;

impl<'de> Visitor<'de> for AcceptSelectorVisitor {
    type Value = AcceptSelector;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string or a sequence of strings")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        AcceptSelector::from_str(v).map_err(serde::de::Error::custom)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut selector = AcceptSelector::new();
        while let Some(value) = seq.next_element::<String>()? {
            selector.add_range(AcceptRange::from_str(&value).map_err(serde::de::Error::custom)?);
        }
        Ok(selector)
    }
}

impl<'de> Deserialize<'de> for AcceptSelector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(AcceptSelectorVisitor)
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
        let selector = AcceptSelector::from_str(input).unwrap();
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
    fn test_deserialize(
        #[case] input: &str,
        #[case] valid_values: Vec<u16>,
        #[case] invalid_values: Vec<u16>,
        #[case] length: usize,
    ) {
        #[derive(Deserialize)]
        struct Config {
            accept: AcceptSelector,
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
    #[case("100..=150,200..=300", "[100..=150,200..=300]")]
    #[case("100..=150,300", "[100..=150,300..=300]")]
    fn test_display(#[case] input: &str, #[case] display: &str) {
        let selector = AcceptSelector::from_str(input).unwrap();
        assert_eq!(selector.to_string(), display)
    }
}
