use governor::Quota;
use humantime_serde::re::humantime::{self, DurationError};
use serde::{Deserialize, Serialize, Serializer};
use std::num::NonZero;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Interval between requests to the same host
pub struct RequestInterval(Quota);

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("Parse error: {0}")]
    HumantimeError(DurationError),
    #[error("Interval must not be zero")]
    ZeroInterval,
}

impl FromStr for RequestInterval {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let duration = input
            .parse::<humantime::Duration>()
            .map_err(ParseError::HumantimeError)?;
        Ok(RequestInterval(
            Quota::with_period(duration.into()).ok_or(ParseError::ZeroInterval)?,
        ))
    }
}

impl RequestInterval {
    /// Convert into inner [`Quota`]
    #[must_use]
    pub const fn into_inner(self) -> Quota {
        self.0
    }
}

impl Default for RequestInterval {
    /// The default interval is 50 milliseconds.
    fn default() -> Self {
        const PER_SECOND: Quota = Quota::per_second(NonZero::new(20).unwrap());
        Self(PER_SECOND)
    }
}

impl Serialize for RequestInterval {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        humantime::Duration::from(self.0.replenish_interval())
            .to_string()
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RequestInterval {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let string = <String>::deserialize(deserializer)?;
        Self::from_str(&string).map_err(serde::de::Error::custom)
    }
}
