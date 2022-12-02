//! Control `log` level with a `--verbose` flag for your CLI
//!
//! The original source is from
//! [this crate](https://github.com/clap-rs/clap-verbosity-flag).
//! Modifications were made to add support for serializing the `Verbosity`
//! struct and to add a convenience method to get the verbosity status.
//!
//! # Examples
//!
//! To get `--quiet` and `--verbose` flags through your entire program, just `flatten`
//! [`Verbosity`]:
//! ```rust,no_run
//! # use clap::Parser;
//! # use clap_verbosity_flag::Verbosity;
//! #
//! # /// Le CLI
//! # #[derive(Debug, Parser)]
//! # struct Cli {
//! #[command(flatten)]
//! verbose: Verbosity,
//! # }
//! ```
//!
//! You can then use this to configure your logger:
//! ```rust,no_run
//! # use clap::Parser;
//! # use clap_verbosity_flag::Verbosity;
//! #
//! # /// Le CLI
//! # #[derive(Debug, Parser)]
//! # struct Cli {
//! #     #[command(flatten)]
//! #     verbose: Verbosity,
//! # }
//! let cli = Cli::parse();
//! env_logger::Builder::new()
//!     .filter_level(cli.verbose.log_level_filter())
//!     .init();
//! ```
//!
//! By default, this will only report errors.
//! - `-q` silences output
//! - `-v` show warnings
//! - `-vv` show info
//! - `-vvv` show debug
//! - `-vvvv` show trace
//!
//! You can also customize the default logging level:
//! ```rust,no_run
//! # use clap::Parser;
//! use clap_verbosity_flag::{Verbosity, InfoLevel};
//!
//! /// Le CLI
//! #[derive(Debug, Parser)]
//! struct Cli {
//!     #[command(flatten)]
//!     verbose: Verbosity<InfoLevel>,
//! }
//! ```
//!
//! Or implement [`LogLevel`] yourself for more control.

#[derive(clap::Args, Eq, PartialEq, Debug, Deserialize, Clone)]
pub(crate) struct Verbosity<L: LogLevel = ErrorLevel> {
    #[clap(
        long,
        short = 'v',
        action = clap::ArgAction::Count,
        global = true,
        help = L::verbose_help(),
        long_help = L::verbose_long_help(),
    )]
    verbose: u8,

    #[clap(
        long,
        short = 'q',
        action = clap::ArgAction::Count,
        global = true,
        help = L::quiet_help(),
        long_help = L::quiet_long_help(),
        conflicts_with = "verbose",
    )]
    quiet: u8,

    #[clap(skip)]
    phantom: std::marker::PhantomData<L>,
}

impl<L: LogLevel> Verbosity<L> {
    /// Create a new verbosity instance by explicitly setting the values
    pub(crate) const fn new(verbose: u8, quiet: u8) -> Self {
        Verbosity {
            verbose,
            quiet,
            phantom: std::marker::PhantomData,
        }
    }

    /// Get the log level.
    ///
    /// `None` means all output is disabled.
    pub(crate) fn log_level(&self) -> Option<log::Level> {
        level_enum(self.verbosity())
    }

    /// Get the log level filter.
    pub(crate) fn log_level_filter(&self) -> log::LevelFilter {
        level_enum(self.verbosity()).map_or(log::LevelFilter::Off, |l| l.to_level_filter())
    }

    /// Shorthand to check if the user requested "more verbose" output
    /// (assuming the default log level is `Warn`)
    pub(crate) fn is_verbose(&self) -> bool {
        self.log_level() > log::LevelFilter::Error.to_level()
    }

    fn verbosity(&self) -> i8 {
        #![allow(clippy::cast_possible_wrap)]
        level_value(L::default()) - (self.quiet as i8) + (self.verbose as i8)
    }
}

const fn level_value(level: Option<log::Level>) -> i8 {
    match level {
        None => -1,
        Some(log::Level::Error) => 0,
        Some(log::Level::Warn) => 1,
        Some(log::Level::Info) => 2,
        Some(log::Level::Debug) => 3,
        Some(log::Level::Trace) => 4,
    }
}

const fn level_enum(verbosity: i8) -> Option<log::Level> {
    match verbosity {
        std::i8::MIN..=-1 => None,
        0 => Some(log::Level::Error),
        1 => Some(log::Level::Warn),
        2 => Some(log::Level::Info),
        3 => Some(log::Level::Debug),
        4..=std::i8::MAX => Some(log::Level::Trace),
    }
}

use std::fmt;

use serde::Deserialize;

impl<L: LogLevel> fmt::Display for Verbosity<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.verbosity())
    }
}

pub(crate) trait LogLevel {
    fn default() -> Option<log::Level>;

    fn verbose_help() -> Option<&'static str> {
        Some("Set verbosity level; more output per occurrence (e.g. `-v` or `-vv`)")
    }

    fn verbose_long_help() -> Option<&'static str> {
        None
    }

    fn quiet_help() -> Option<&'static str> {
        Some("Less output per occurrence (e.g. `-q` or `-qq`)")
    }

    fn quiet_long_help() -> Option<&'static str> {
        None
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub(crate) struct ErrorLevel;

impl LogLevel for ErrorLevel {
    fn default() -> Option<log::Level> {
        Some(log::Level::Error)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub(crate) struct WarnLevel;

impl LogLevel for WarnLevel {
    fn default() -> Option<log::Level> {
        Some(log::Level::Warn)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub(crate) struct InfoLevel;

impl LogLevel for InfoLevel {
    fn default() -> Option<log::Level> {
        Some(log::Level::Info)
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
