use std::{collections::HashSet, fmt::Display, str::FromStr};

use serde::{Deserialize, de::Visitor};

use crate::{
    AcceptRangeError, types::accept::AcceptRange, types::status_code::StatusCodeSelectorError,
};

/// A [`StatusCodeExcluder`] holds ranges of HTTP status codes, and determines
/// whether a specific code is matched, so the link can be counted as valid (not
/// broken) or excluded. `StatusCodeExcluder` differs from
/// [`StatusCodeSelector`](super::selector::StatusCodeSelector) in the defaults
/// it provides. As this is meant to exclude status codes, the default is to
/// keep everything.
#[derive(Clone, Debug, PartialEq)]
pub struct StatusCodeExcluder {
    ranges: Vec<AcceptRange>,
}

impl FromStr for StatusCodeExcluder {
    type Err = StatusCodeSelectorError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();

        if input.is_empty() {
            return Ok(Self::new());
        }

        let ranges = input
            .split(',')
            .map(|part| AcceptRange::from_str(part.trim()))
            .collect::<Result<Vec<AcceptRange>, AcceptRangeError>>()?;

        Ok(Self::new_from(ranges))
    }
}

impl Default for StatusCodeExcluder {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for StatusCodeExcluder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ranges: Vec<_> = self.ranges.iter().map(ToString::to_string).collect();
        write!(f, "{}", ranges.join(","))
    }
}

impl StatusCodeExcluder {
    /// Creates a new empty [`StatusCodeExcluder`].
    #[must_use]
    pub const fn new() -> Self {
        Self { ranges: Vec::new() }
    }

    /// Creates a new [`StatusCodeExcluder`] prefilled with `ranges`.
    #[must_use]
    pub fn new_from(ranges: Vec<AcceptRange>) -> Self {
        let mut selector = Self::new();

        for range in ranges {
            selector.add_range(range);
        }

        selector
    }

    /// Adds a range of HTTP status codes to this [`StatusCodeExcluder`].
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

    /// Returns whether this [`StatusCodeExcluder`] contains `value`.
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
    pub(crate) const fn len(&self) -> usize {
        self.ranges.len()
    }
}

struct StatusCodeExcluderVisitor;

impl<'de> Visitor<'de> for StatusCodeExcluderVisitor {
    type Value = StatusCodeExcluder;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string or a sequence of strings")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        StatusCodeExcluder::from_str(v).map_err(serde::de::Error::custom)
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let value = u16::try_from(v).map_err(serde::de::Error::custom)?;
        Ok(StatusCodeExcluder::new_from(vec![AcceptRange::new(
            value, value,
        )]))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut selector = StatusCodeExcluder::new();
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

impl<'de> Deserialize<'de> for StatusCodeExcluder {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StatusCodeExcluderVisitor)
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
        let selector = StatusCodeExcluder::from_str(input).unwrap();
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
            accept: StatusCodeExcluder,
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
        let selector = StatusCodeExcluder::from_str(input).unwrap();
        assert_eq!(selector.to_string(), display);
    }
}
