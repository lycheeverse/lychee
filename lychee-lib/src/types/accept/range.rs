use std::{num::ParseIntError, ops::RangeInclusive, str::FromStr};

use once_cell::sync::Lazy;
use regex::Regex;
use thiserror::Error;

static RANGE_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^([0-9]*)?\.\.(=?)([0-9]+)+$").unwrap());

/// The [`AcceptRangeParseError`] indicates that the parsing process of an
/// [`AcceptRange`]  from a string failed due to various underlying reasons.
#[derive(Debug, Error, PartialEq)]
pub enum AcceptRangeParseError {
    /// The string input didn't contain any range pattern.
    #[error("No range pattern found")]
    NoRangePattern,

    /// The start or end index could not be parsed as an integer.
    #[error("Failed to parse str as integer")]
    ParseIntError(#[from] ParseIntError),

    /// The start index is larger than the end index.
    #[error("Invalid range indices, only start < end supported")]
    InvalidRangeIndices,
}

/// [`AcceptRange`] specifies which HTTP status codes are accepted and
/// considered successful when checking a remote URL.
#[derive(Debug, PartialEq)]
pub struct AcceptRange(RangeInclusive<usize>);

impl FromStr for AcceptRange {
    type Err = AcceptRangeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let captures = RANGE_PATTERN
            .captures(s)
            .ok_or(AcceptRangeParseError::NoRangePattern)?;

        let inclusive = !captures[2].is_empty();

        let start: usize = captures[1].parse().unwrap_or_default();
        let end: usize = captures[3].parse()?;

        if start >= end {
            return Err(AcceptRangeParseError::InvalidRangeIndices);
        }

        if inclusive {
            Ok(Self::new(start, end))
        } else {
            Ok(Self::new(start, end - 1))
        }
    }
}

impl AcceptRange {
    /// Creates a new [`AcceptRange`] which matches values between `start` and
    /// `end` (both inclusive).
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self(RangeInclusive::new(start, end))
    }

    /// Returns the `start` value of this [`AcceptRange`].
    #[must_use]
    pub const fn start(&self) -> &usize {
        self.0.start()
    }

    /// Returns the `end` value of this [`AcceptRange`].
    #[must_use]
    pub const fn end(&self) -> &usize {
        self.0.end()
    }

    /// Returns whether this [`AcceptRange`] contains `value`.
    #[must_use]
    pub fn contains(&self, value: usize) -> bool {
        self.0.contains(&value)
    }

    pub(crate) fn update_start(&mut self, new_start: usize) {
        self.0 = RangeInclusive::new(new_start, *self.end());
    }

    pub(crate) fn update_end(&mut self, new_end: usize) {
        self.0 = RangeInclusive::new(*self.start(), new_end);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("0..=10", vec![0, 1, 4, 5, 10], vec![11, 12])]
    #[case("..=10", vec![0, 1, 4, 9, 10], vec![11, 12])]
    #[case("0..10", vec![0, 1, 4, 5, 9], vec![10, 11])]
    #[case("..10", vec![0, 1, 4, 9], vec![10, 11])]
    fn test_from_str(
        #[case] input: &str,
        #[case] valid_values: Vec<usize>,
        #[case] invalid_values: Vec<usize>,
    ) {
        let range = AcceptRange::from_str(input).unwrap();

        for valid in valid_values {
            assert!(range.contains(valid));
        }

        for invalid in invalid_values {
            assert!(!range.contains(invalid));
        }
    }

    #[rstest]
    #[case("10..=0", AcceptRangeParseError::InvalidRangeIndices)]
    #[case("0..0", AcceptRangeParseError::InvalidRangeIndices)]
    #[case("-1..=10", AcceptRangeParseError::NoRangePattern)]
    #[case("-1..10", AcceptRangeParseError::NoRangePattern)]
    #[case("0..=-1", AcceptRangeParseError::NoRangePattern)]
    #[case("0..-1", AcceptRangeParseError::NoRangePattern)]
    #[case("abcd", AcceptRangeParseError::NoRangePattern)]
    #[case("-1", AcceptRangeParseError::NoRangePattern)]
    #[case("0", AcceptRangeParseError::NoRangePattern)]
    fn test_from_str_invalid(#[case] input: &str, #[case] error: AcceptRangeParseError) {
        let range = AcceptRange::from_str(input);
        assert_eq!(range, Err(error));
    }
}
