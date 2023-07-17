use std::{num::ParseIntError, ops::RangeInclusive, str::FromStr};

use once_cell::sync::Lazy;
use regex::Regex;
use thiserror::Error;

static RANGE_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"([0-9]*)?..(=?)([0-9]+)+").unwrap());

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

        match inclusive {
            true => Ok(Self::new(start, end)),
            false => Ok(Self::new(start, end - 1)),
        }
    }
}

impl AcceptRange {
    /// Creates a new [`AcceptRange`] which matches values between `start` and
    /// `end` (both inclusive).
    pub fn new(start: usize, end: usize) -> Self {
        Self(RangeInclusive::new(start, end))
    }

    /// Returns the `start` value of this [`AcceptRange`].
    pub fn start(&self) -> &usize {
        self.0.start()
    }

    /// Returns the `end` value of this [`AcceptRange`].
    pub fn end(&self) -> &usize {
        self.0.end()
    }

    /// Returns whether this [`AcceptRange`] contains `value`.
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

    #[test]
    fn test_from_str() {
        let range = AcceptRange::from_str("0..10").unwrap();

        assert!(range.contains(0));
        assert!(range.contains(9));
        assert!(!range.contains(10));
    }

    #[test]
    fn test_from_str_inclusive() {
        let range = AcceptRange::from_str("0..=10").unwrap();

        assert!(range.contains(0));
        assert!(range.contains(9));
        assert!(range.contains(10));
        assert!(!range.contains(11));
    }

    #[test]
    fn test_from_str_open_start() {
        let range = AcceptRange::from_str("..10").unwrap();

        assert!(range.contains(0));
        assert!(range.contains(9));
        assert!(!range.contains(10));
    }

    #[test]
    fn test_from_str_open_start_inclusive() {
        let range = AcceptRange::from_str("..=10").unwrap();

        assert!(range.contains(0));
        assert!(range.contains(9));
        assert!(range.contains(10));
        assert!(!range.contains(11));
    }

    #[test]
    fn test_from_str_invalid_indices() {
        let range = AcceptRange::from_str("10..=0");
        assert_eq!(range, Err(AcceptRangeParseError::InvalidRangeIndices))
    }
}
