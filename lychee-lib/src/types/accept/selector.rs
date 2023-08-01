use std::{collections::HashSet, str::FromStr};

use serde_with::DeserializeFromStr;
use thiserror::Error;

use crate::{types::accept::AcceptRange, AcceptRangeError};

#[derive(Debug, Error)]
pub enum AcceptSelectorError {
    #[error("Invalid/empty input")]
    InvalidInput,

    #[error("Failed to parse accept range")]
    AcceptRangeError(#[from] AcceptRangeError),
}

/// An [`AcceptSelector`] determines if a returned HTTP status code should be
/// accepted and thus counted as a valid (not broken) link.
#[derive(Clone, Debug, Default, DeserializeFromStr, PartialEq)]
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

impl AcceptSelector {
    /// Creates a new empty [`AcceptSelector`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
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

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("1..=10,20..=30", vec![1, 10, 20, 30], vec![15, 35], 2)]
    #[case("1..=10,8..=20", vec![1, 15, 20], vec![25, 30], 1)]
    #[case("8..=20,1..=10", vec![1, 15, 20], vec![25, 30], 1)]
    #[case("1..=10,20", vec![1, 10, 20], vec![15, 25], 2)]
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
}
