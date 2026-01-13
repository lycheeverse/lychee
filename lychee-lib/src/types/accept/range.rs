use std::{fmt::Display, num::ParseIntError, ops::RangeInclusive, str::FromStr, sync::LazyLock};

use regex::Regex;
use thiserror::Error;

/// Smallest accepted value
const MIN: u16 = 100;

/// Biggest accepted value
const MAX: u16 = 999;

static RANGE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([0-9]+)?\.\.((=?)([0-9]+))?$|^([0-9]+)$").unwrap());

/// Indicates that the parsing process of an [`AcceptRange`]  from a string
/// failed due to various underlying reasons.
#[derive(Debug, Error, PartialEq)]
pub enum AcceptRangeError {
    /// The string input didn't contain any range pattern.
    #[error("no range pattern found")]
    NoRangePattern,

    /// The start or end index could not be parsed as an integer.
    #[error("failed to parse str as integer")]
    ParseIntError(#[from] ParseIntError),

    /// The start index is larger than the end index.
    #[error("invalid range indices, only start < end supported")]
    InvalidRangeIndices,

    /// The u16 values must be representable as status code
    #[error("values must represent valid status codes between 100 and 999 (inclusive)")]
    InvalidStatusCodeValue,
}

/// [`AcceptRange`] specifies which HTTP status codes are accepted and
/// considered successful when checking a remote URL.
/// Only represents valid status codes,
/// invalid status codes (<100 or >999) are rejected.
#[derive(Clone, Debug, PartialEq)]
pub struct AcceptRange(RangeInclusive<u16>);

impl FromStr for AcceptRange {
    type Err = AcceptRangeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let captures = RANGE_PATTERN
            .captures(s)
            .ok_or(AcceptRangeError::NoRangePattern)?;

        if let Some(value) = captures.get(5) {
            let value: u16 = value.as_str().parse()?;
            Self::new(value, value)
        } else {
            let start: u16 = match captures.get(1) {
                Some(start) => start.as_str().parse().unwrap_or_default(),
                None => MIN,
            };
            if captures.get(2).is_none() {
                return Self::new(start, MAX);
            }

            let inclusive = !captures[3].is_empty();
            let end: u16 = captures[4].parse()?;

            if inclusive {
                Self::new(start, end)
            } else {
                Self::new(start, end - 1)
            }
        }
    }
}

impl Display for AcceptRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}..={}", self.start(), self.end())
    }
}

impl AcceptRange {
    /// Creates a new [`AcceptRange`] which matches values between `start` and
    /// `end` (both inclusive).
    ///
    /// # Errors
    ///
    /// Returns an error if `start` > `end`, `start` < 100 or `end` > 999.
    pub const fn new(start: u16, end: u16) -> Result<Self, AcceptRangeError> {
        if start < MIN || end > MAX {
            return Err(AcceptRangeError::InvalidStatusCodeValue);
        }

        if start > end {
            return Err(AcceptRangeError::InvalidRangeIndices);
        }

        Ok(Self(RangeInclusive::new(start, end)))
    }

    /// Returns the `start` value of this [`AcceptRange`].
    #[must_use]
    pub const fn start(&self) -> &u16 {
        self.0.start()
    }

    /// Returns the `end` value of this [`AcceptRange`].
    #[must_use]
    pub const fn end(&self) -> &u16 {
        self.0.end()
    }

    /// Returns whether this [`AcceptRange`] contains `value`.
    #[must_use]
    pub fn contains(&self, value: u16) -> bool {
        self.0.contains(&value)
    }

    /// Consumes self and returns the inner range.
    #[must_use]
    pub const fn inner(self) -> RangeInclusive<u16> {
        self.0
    }

    pub(crate) const fn update_start(&mut self, new_start: u16) -> Result<(), AcceptRangeError> {
        // Can't use `?` in const function as of 1.91.0
        match Self::new(new_start, *self.end()) {
            Ok(r) => {
                self.0 = r.0;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub(crate) const fn update_end(&mut self, new_end: u16) -> Result<(), AcceptRangeError> {
        // Can't use `?` in const function as of 1.91.0
        match Self::new(*self.start(), new_end) {
            Ok(r) => {
                self.0 = r.0;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub(crate) fn merge(&mut self, other: &Self) -> bool {
        // Merge when the end value of self overlaps with other's start
        if self.end() >= other.start() && other.end() >= self.end() {
            // We can ignore the result here, as it is guaranteed that
            // start < new_end
            let _ = self.update_end(*other.end());
            return true;
        }

        // Merge when the start value of self overlaps with other's end
        if self.start() <= other.end() && other.start() <= self.start() {
            // We can ignore the result here, as it is guaranteed that
            // start < new_end
            let _ = self.update_start(*other.start());
            return true;
        }

        false
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("..", vec![MIN, 150, 200, MAX], vec![MIN - 1, MAX + 1])]
    #[case("100..", vec![100, 101, 150, 200, MAX], vec![0, 50, 99])]
    #[case("100..=200", vec![100, 150, 200], vec![0, 50, 99, 201, 250, MAX+1])]
    #[case("..=100", vec![100], vec![99, 101])]
    #[case("100..200", vec![100, 150, 199], vec![99, 200, 250])]
    #[case("..101", vec![100], vec![99, 101])]
    #[case("404", vec![404], vec![200, 304, 403, 405, 500])]
    fn test_from_str(
        #[case] input: &str,
        #[case] included_values: Vec<u16>,
        #[case] excluded_values: Vec<u16>,
    ) {
        let range = AcceptRange::from_str(input).unwrap();

        for included in included_values {
            assert!(range.contains(included));
        }

        for excluded in excluded_values {
            assert!(!range.contains(excluded));
        }
    }

    #[rstest]
    #[case("..100", AcceptRangeError::InvalidRangeIndices)]
    #[case("200..=100", AcceptRangeError::InvalidRangeIndices)]
    #[case("100..100", AcceptRangeError::InvalidRangeIndices)]
    #[case("..=", AcceptRangeError::NoRangePattern)]
    #[case("100..=", AcceptRangeError::NoRangePattern)]
    #[case("-100..=100", AcceptRangeError::NoRangePattern)]
    #[case("-100..100", AcceptRangeError::NoRangePattern)]
    #[case("100..=-100", AcceptRangeError::NoRangePattern)]
    #[case("100..-100", AcceptRangeError::NoRangePattern)]
    #[case("abcd", AcceptRangeError::NoRangePattern)]
    #[case("-1", AcceptRangeError::NoRangePattern)]
    #[case("0", AcceptRangeError::InvalidStatusCodeValue)]
    #[case("1..5", AcceptRangeError::InvalidStatusCodeValue)]
    #[case("99..102", AcceptRangeError::InvalidStatusCodeValue)]
    #[case("999..=1000", AcceptRangeError::InvalidStatusCodeValue)]
    fn test_from_str_invalid(#[case] input: &str, #[case] error: AcceptRangeError) {
        let range = AcceptRange::from_str(input);
        assert_eq!(range, Err(error));
    }

    #[rstest]
    #[case("100..=200", "210..=300", "100..=200")]
    #[case("100..=200", "190..=300", "100..=300")]
    #[case("100..200", "200..300", "100..200")]
    #[case("100..200", "190..300", "100..300")]
    fn test_merge(#[case] range: &str, #[case] other: &str, #[case] result: &str) {
        let mut range = AcceptRange::from_str(range).unwrap();
        let other = AcceptRange::from_str(other).unwrap();

        let result = AcceptRange::from_str(result).unwrap();
        range.merge(&other);

        assert_eq!(result, range);
    }
}
