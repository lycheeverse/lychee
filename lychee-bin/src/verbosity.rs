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

#[derive(clap::Args, Debug, Clone, PartialEq, Eq)]
pub(crate) struct Verbosity {
    /// Pass many times for more log output
    ///
    /// By default, it'll only report errors. Passing `-v` one time also prints
    /// warnings, `-vv` enables info logging, `-vvv` debug, and `-vvvv` trace.
    #[arg(long = "verbose", short = 'v', action = clap::ArgAction::Count, global = true)]
    verbosity: u8,
}

impl Verbosity {
    /// Create a new verbosity instance by explicitly setting the values
    pub(crate) const fn new(verbosity: u8) -> Verbosity {
        Verbosity { verbosity }
    }

    /// Get the log level.
    ///
    /// `None` means all output is disabled.
    pub(crate) const fn log_level(&self) -> Level {
        level_enum(self.verbosity)
    }

    /// Get the log level filter.
    pub(crate) fn log_level_filter(&self) -> LevelFilter {
        level_enum(self.verbosity).to_level_filter()
    }
}

// Implement Deserialize for `Verbosity`
// This can be deserialized from a string like "warn", "warning", or "Warning"
// for example
impl<'de> Deserialize<'de> for Verbosity {
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
            verbosity: level_value(level),
        })
    }
}

const fn level_value(level: Level) -> u8 {
    match level {
        log::Level::Error => 0,
        log::Level::Warn => 1,
        log::Level::Info => 2,
        log::Level::Debug => 3,
        log::Level::Trace => 4,
    }
}

const fn level_enum(verbosity: u8) -> Level {
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
        write!(f, "{}", self.verbosity)
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
}
