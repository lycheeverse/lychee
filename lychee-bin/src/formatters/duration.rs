use std::fmt;

/// Wrapper struct around `elapsed.as_secs()` for custom formatting.
///
/// # Examples
///
/// ```
/// use lychee_bin::formatters::Duration;
///
/// let duration = Duration::from_secs(1);
/// assert_eq!(duration.to_string(), "1s");
///
/// let duration = Duration::from_secs(60);
/// assert_eq!(duration.to_string(), "1m");
///
/// let duration = Duration::from_secs(61);
/// assert_eq!(duration.to_string(), "1m 1s");
///
/// let duration = Duration::from_secs(3661);
/// assert_eq!(duration.to_string(), "1h 1m 1s");
/// ```
pub(crate) struct Duration {
    elapsed: u64,
}

impl Duration {
    /// Create a new `Duration` from the given number of seconds.
    pub(crate) const fn from_secs(elapsed: u64) -> Self {
        Self { elapsed }
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let days = self.elapsed / 86400;
        let hours = (self.elapsed % 86400) / 3600;
        let minutes = (self.elapsed % 3600) / 60;
        let seconds = self.elapsed % 60;

        if days > 0 {
            write!(f, "{days}d {hours}h {minutes}m {seconds}s")
        } else if hours > 0 {
            write!(f, "{hours}h {minutes}m {seconds}s")
        } else if minutes > 0 {
            write!(f, "{minutes}m {seconds}s")
        } else {
            write!(f, "{seconds}s")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formatting() {
        let duration = Duration::from_secs(61);
        assert_eq!(duration.to_string(), "1m 1s");

        let duration = Duration::from_secs(3661);
        assert_eq!(duration.to_string(), "1h 1m 1s");

        let duration = Duration::from_secs(90061);
        assert_eq!(duration.to_string(), "1d 1h 1m 1s");

        let duration = Duration::from_secs(0);
        assert_eq!(duration.to_string(), "0s");

        // 100 years printed as days
        let duration = Duration::from_secs(3_153_600_000);
        assert_eq!(duration.to_string(), "36500d 0h 0m 0s");
    }
}
