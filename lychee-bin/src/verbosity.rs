//! Easily add a `--verbose` flag to CLIs using Structopt
//!
//! # Examples
//!
//! ```rust,no_run
//! use clap::Parser;
//! use clap_verbosity_flag::Verbosity;
//!
//! /// Le CLI
//! #[derive(Debug, Parser)]
//! struct Cli {
//!     #[command(flatten)]
//!     verbose: Verbosity,
//! }
//!
//! let cli = Cli::parse();
//! env_logger::Builder::new()
//!     .filter_level(cli.verbose.log_level_filter())
//!     .init();
//! ```
//!
//! This will only report errors.
//! - `-q` silences output
//! - `-v` show warnings
//! - `-vv` show info
//! - `-vvv` show debug
//! - `-vvvv` show trace

use log::Level;
use log::LevelFilter;
use serde::Deserialize;

#[derive(clap::Args, Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Verbosity {
    /// Pass many times for more log output
    ///
    /// By default, it'll only report errors and warnings.
    /// Passing `-v` one time also prints info,
    /// `-vv` enables debugging logging, `-vvv` trace.
    #[arg(
        long,
        short = 'v',
        action = clap::ArgAction::Count,
        global = true,
        help = Self::verbose_help(),
        long_help = Self::verbose_long_help(),
        conflicts_with = "quiet",
    )]
    verbose: u8,

    #[arg(
        long,
        short = 'q',
        action = clap::ArgAction::Count,
        global = true,
        help = Self::quiet_help(),
        long_help = Self::quiet_long_help(),
        conflicts_with = "verbose",
    )]
    quiet: u8,
}

impl Verbosity {
    /// Get the log level.
    ///
    /// `None` means all output is disabled.
    pub(crate) const fn log_level(&self) -> Level {
        level_enum(self.verbosity())
    }

    /// Get the log level filter.
    pub(crate) fn log_level_filter(&self) -> LevelFilter {
        level_enum(self.verbosity()).to_level_filter()
    }

    #[allow(clippy::cast_possible_wrap)]
    const fn verbosity(&self) -> i8 {
        level_value(log::Level::Warn) - (self.quiet as i8) + (self.verbose as i8)
    }

    const fn verbose_help() -> &'static str {
        "Set verbosity level; more output per occurrence (e.g. `-v` or `-vv`)"
    }

    const fn verbose_long_help() -> Option<&'static str> {
        None
    }

    const fn quiet_help() -> &'static str {
        "Less output per occurrence (e.g. `-q` or `-qq`)"
    }

    const fn quiet_long_help() -> Option<&'static str> {
        None
    }
}

#[cfg(test)]
impl Verbosity {
    pub(crate) const fn debug() -> Self {
        Self {
            #[allow(clippy::cast_sign_loss)]
            verbose: level_value(log::Level::Debug) as u8,
            quiet: 0,
        }
    }
}

// Implement Deserialize for `Verbosity`
// This can be deserialized from a string like "warn", "warning", or "Warning"
// for example
impl<'de> Deserialize<'de> for Verbosity {
    #[allow(clippy::cast_sign_loss)]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let level = match s.to_lowercase().as_str() {
            "error" => Level::Error,
            "warn" | "warning" => Level::Warn,
            "info" => Level::Info,
            "debug" => Level::Debug,
            "trace" => Level::Trace,
            level => {
                return Err(serde::de::Error::custom(format!(
                    "invalid log level `{level}`"
                )))
            }
        };
        Ok(Verbosity {
            verbose: level_value(level) as u8,
            quiet: 0,
        })
    }
}

const fn level_value(level: Level) -> i8 {
    match level {
        log::Level::Error => 0,
        log::Level::Warn => 1,
        log::Level::Info => 2,
        log::Level::Debug => 3,
        log::Level::Trace => 4,
    }
}

const fn level_enum(verbosity: i8) -> log::Level {
    match verbosity {
        0 => log::Level::Error,
        1 => log::Level::Warn,
        2 => log::Level::Info,
        3 => log::Level::Debug,
        _ => log::Level::Trace,
    }
}

use std::fmt;

impl fmt::Display for Verbosity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.verbose)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn verify_app() {
        #[derive(Debug, clap::Parser)]
        struct Cli {
            #[clap(flatten)]
            verbose: Verbosity,
        }

        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn test_default_log_level() {
        let verbosity = Verbosity::default();
        assert_eq!(verbosity.log_level(), Level::Warn);
        assert!(verbosity.log_level() >= Level::Warn);
    }
}
