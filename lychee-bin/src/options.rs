use crate::archive::Archive;
use crate::parse::parse_base;
use crate::verbosity::Verbosity;
use anyhow::{anyhow, Context, Error, Result};
use clap::builder::PossibleValuesParser;
use clap::{arg, builder::TypedValueParser, Parser};
use const_format::{concatcp, formatcp};
use lychee_lib::{
    Base, BasicAuthSelector, Input, StatusCodeExcluder, StatusCodeSelector, DEFAULT_MAX_REDIRECTS,
    DEFAULT_MAX_RETRIES, DEFAULT_RETRY_WAIT_TIME_SECS, DEFAULT_TIMEOUT_SECS, DEFAULT_USER_AGENT,
};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::path::Path;
use std::{fs, path::PathBuf, str::FromStr, time::Duration};
use strum::{Display, EnumIter, EnumString, VariantNames};

pub(crate) const LYCHEE_IGNORE_FILE: &str = ".lycheeignore";
pub(crate) const LYCHEE_CACHE_FILE: &str = ".lycheecache";
pub(crate) const LYCHEE_CONFIG_FILE: &str = "lychee.toml";

const DEFAULT_METHOD: &str = "get";
const DEFAULT_MAX_CACHE_AGE: &str = "1d";
const DEFAULT_MAX_CONCURRENCY: usize = 128;

// this exists because clap requires `&str` type values for defaults
// whereas serde expects owned `String` types
// (we can't use e.g. `TIMEOUT` or `timeout()` which gets created for serde)
const MAX_CONCURRENCY_STR: &str = concatcp!(DEFAULT_MAX_CONCURRENCY);
const MAX_CACHE_AGE_STR: &str = concatcp!(DEFAULT_MAX_CACHE_AGE);
const MAX_REDIRECTS_STR: &str = concatcp!(DEFAULT_MAX_REDIRECTS);
const MAX_RETRIES_STR: &str = concatcp!(DEFAULT_MAX_RETRIES);
const HELP_MSG_CACHE: &str = formatcp!(
    "Use request cache stored on disk at `{}`",
    LYCHEE_CACHE_FILE,
);
// We use a custom help message here because we want to show the default
// value of the config file, but also be able to check if the user has
// provided a custom value. If they didn't, we won't throw an error if
// the file doesn't exist.
const HELP_MSG_CONFIG_FILE: &str = formatcp!(
    "Configuration file to use\n\n[default: {}]",
    LYCHEE_CONFIG_FILE,
);
const TIMEOUT_STR: &str = concatcp!(DEFAULT_TIMEOUT_SECS);
const RETRY_WAIT_TIME_STR: &str = concatcp!(DEFAULT_RETRY_WAIT_TIME_SECS);

/// The format to use for the final status report
#[derive(Debug, Deserialize, Default, Clone, Display, EnumIter, VariantNames, PartialEq)]
#[non_exhaustive]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub(crate) enum StatsFormat {
    #[default]
    Compact,
    Detailed,
    Json,
    Markdown,
    Raw,
}

impl FromStr for StatsFormat {
    type Err = Error;

    fn from_str(format: &str) -> Result<Self, Self::Err> {
        match format.to_lowercase().as_str() {
            "compact" | "string" => Ok(StatsFormat::Compact),
            "detailed" => Ok(StatsFormat::Detailed),
            "json" => Ok(StatsFormat::Json),
            "markdown" | "md" => Ok(StatsFormat::Markdown),
            "raw" => Ok(StatsFormat::Raw),
            _ => Err(anyhow!("Unknown format {}", format)),
        }
    }
}

/// The different formatter modes
///
/// This decides over whether to use color,
/// emojis, or plain text for the output.
#[derive(Debug, Deserialize, Default, Clone, Display, EnumIter, EnumString, VariantNames)]
#[non_exhaustive]
pub(crate) enum OutputMode {
    /// Plain text output.
    ///
    /// This is the most basic output mode for terminals that do not support
    /// color or emojis. It can also be helpful for scripting or when you want
    /// to pipe the output to another program.
    #[serde(rename = "plain")]
    #[strum(serialize = "plain", ascii_case_insensitive)]
    Plain,

    /// Colorful output.
    ///
    /// This mode uses colors to highlight the status of the requests.
    /// It is useful for terminals that support colors and you want to
    /// provide a more visually appealing output.
    ///
    /// This is the default output mode.
    #[serde(rename = "color")]
    #[strum(serialize = "color", ascii_case_insensitive)]
    #[default]
    Color,

    /// Emoji output.
    ///
    /// This mode uses emojis to represent the status of the requests.
    /// Some people may find this mode more intuitive and fun to use.
    #[serde(rename = "emoji")]
    #[strum(serialize = "emoji", ascii_case_insensitive)]
    Emoji,
}

impl OutputMode {
    /// Returns `true` if the response format is `Plain`
    pub(crate) const fn is_plain(&self) -> bool {
        matches!(self, OutputMode::Plain)
    }

    /// Returns `true` if the response format is `Emoji`
    pub(crate) const fn is_emoji(&self) -> bool {
        matches!(self, OutputMode::Emoji)
    }
}

// Macro for generating default functions to be used by serde
macro_rules! default_function {
    ( $( $name:ident : $T:ty = $e:expr; )* ) => {
        $(
            #[allow(clippy::missing_const_for_fn)]
            fn $name() -> $T {
                $e
            }
        )*
    };
}

// Generate the functions for serde defaults
default_function! {
    max_redirects: usize = DEFAULT_MAX_REDIRECTS;
    max_retries: u64 = DEFAULT_MAX_RETRIES;
    max_concurrency: usize = DEFAULT_MAX_CONCURRENCY;
    max_cache_age: Duration = humantime::parse_duration(DEFAULT_MAX_CACHE_AGE).unwrap();
    user_agent: String = DEFAULT_USER_AGENT.to_string();
    timeout: usize = DEFAULT_TIMEOUT_SECS;
    retry_wait_time: usize = DEFAULT_RETRY_WAIT_TIME_SECS;
    method: String = DEFAULT_METHOD.to_string();
    verbosity: Verbosity = Verbosity::default();
    cache_exclude_selector: StatusCodeExcluder = StatusCodeExcluder::new();
    accept_selector: StatusCodeSelector = StatusCodeSelector::default();
}

// Macro for merging configuration values
macro_rules! fold_in {
    ( $cli:ident , $toml:ident ; $( $key:ident : $default:expr; )* ) => {
        $(
            if $cli.$key == $default && $toml.$key != $default {
                $cli.$key = $toml.$key;
            }
        )*
    };
}

/// A fast, async link checker
///
/// Finds broken URLs and mail addresses inside Markdown, HTML,
/// `reStructuredText`, websites and more!
#[derive(Parser, Debug)]
#[command(version, about)]
pub(crate) struct LycheeOptions {
    /// The inputs (where to get links to check from).
    /// These can be: files (e.g. `README.md`), file globs (e.g. `"~/git/*/README.md"`),
    /// remote URLs (e.g. `https://example.com/README.md`) or standard input (`-`).
    /// NOTE: Use `--` to separate inputs from options that allow multiple arguments.
    #[arg(name = "inputs", required = true)]
    raw_inputs: Vec<String>,

    /// Configuration file to use
    #[arg(short, long = "config")]
    #[arg(help = HELP_MSG_CONFIG_FILE)]
    pub(crate) config_file: Option<PathBuf>,

    #[clap(flatten)]
    pub(crate) config: Config,
}

impl LycheeOptions {
    /// Get parsed inputs from options.
    // This depends on the config, which is why a method is required (we could
    // accept a `Vec<Input>` in `LycheeOptions` and do the conversion there, but
    // we wouldn't get access to `glob_ignore_case`.
    pub(crate) fn inputs(&self) -> Result<Vec<Input>> {
        let excluded = if self.config.exclude_path.is_empty() {
            None
        } else {
            Some(self.config.exclude_path.clone())
        };
        self.raw_inputs
            .iter()
            .map(|s| Input::new(s, None, self.config.glob_ignore_case, excluded.clone()))
            .collect::<Result<_, _>>()
            .context("Cannot parse inputs from arguments")
    }
}

/// The main configuration for lychee
#[allow(clippy::struct_excessive_bools)]
#[derive(Parser, Debug, Deserialize, Clone, Default)]
pub(crate) struct Config {
    /// Verbose program output
    #[clap(flatten)]
    #[serde(default = "verbosity")]
    pub(crate) verbose: Verbosity,

    /// Do not show progress bar.
    /// This is recommended for non-interactive shells (e.g. for continuous integration)
    #[arg(short, long, verbatim_doc_comment)]
    #[serde(default)]
    pub(crate) no_progress: bool,

    #[arg(help = HELP_MSG_CACHE)]
    #[arg(long)]
    #[serde(default)]
    pub(crate) cache: bool,

    /// Discard all cached requests older than this duration
    #[arg(
        long,
        value_parser = humantime::parse_duration,
        default_value = &MAX_CACHE_AGE_STR
    )]
    #[serde(default = "max_cache_age")]
    #[serde(with = "humantime_serde")]
    pub(crate) max_cache_age: Duration,

    /// A list of status codes that will be excluded from the cache
    #[arg(
        long,
        default_value_t,
        long_help = "A list of status codes that will be ignored from the cache

The following accept range syntax is supported: [start]..[=]end|code. Some valid
examples are:

- 429
- 500..=599
- 500..

Use \"lychee --cache-exclude-status '429, 500..502' <inputs>...\" to provide a comma- separated
list of excluded status codes. This example will not cache results with a status code of 429, 500,
501 and 502."
    )]
    #[serde(default = "cache_exclude_selector")]
    pub(crate) cache_exclude_status: StatusCodeExcluder,

    /// Don't perform any link checking.
    /// Instead, dump all the links extracted from inputs that would be checked
    #[arg(long)]
    #[serde(default)]
    pub(crate) dump: bool,

    /// Don't perform any link extraction and checking.
    /// Instead, dump all input sources from which links would be collected
    #[arg(long)]
    #[serde(default)]
    pub(crate) dump_inputs: bool,

    /// Specify the use of a specific web archive.
    /// Can be used in combination with `--suggest`
    #[arg(long, value_parser = PossibleValuesParser::new(Archive::VARIANTS).map(|s| s.parse::<Archive>().unwrap()))]
    #[serde(default)]
    pub(crate) archive: Option<Archive>,

    /// Suggest link replacements for broken links, using a web archive.
    /// The web archive can be specified with `--archive`
    #[arg(long)]
    #[serde(default)]
    pub(crate) suggest: bool,

    /// Maximum number of allowed redirects
    #[arg(short, long, default_value = &MAX_REDIRECTS_STR)]
    #[serde(default = "max_redirects")]
    pub(crate) max_redirects: usize,

    /// Maximum number of retries per request
    #[arg(long, default_value = &MAX_RETRIES_STR)]
    #[serde(default = "max_retries")]
    pub(crate) max_retries: u64,

    /// Maximum number of concurrent network requests
    #[arg(long, default_value = &MAX_CONCURRENCY_STR)]
    #[serde(default = "max_concurrency")]
    pub(crate) max_concurrency: usize,

    /// Number of threads to utilize.
    /// Defaults to number of cores available to the system
    #[arg(short = 'T', long)]
    #[serde(default)]
    pub(crate) threads: Option<usize>,

    /// User agent
    #[arg(short, long, default_value = DEFAULT_USER_AGENT)]
    #[serde(default = "user_agent")]
    pub(crate) user_agent: String,

    /// Proceed for server connections considered insecure (invalid TLS)
    #[arg(short, long)]
    #[serde(default)]
    pub(crate) insecure: bool,

    /// Only test links with the given schemes (e.g. https).
    /// Omit to check links with any other scheme.
    /// At the moment, we support http, https, file, and mailto.
    #[arg(short, long)]
    #[serde(default)]
    pub(crate) scheme: Vec<String>,

    /// Only check local files and block network requests.
    #[arg(long)]
    #[serde(default)]
    pub(crate) offline: bool,

    /// URLs to check (supports regex). Has preference over all excludes.
    #[arg(long)]
    #[serde(default)]
    pub(crate) include: Vec<String>,

    /// Exclude URLs and mail addresses from checking (supports regex)
    #[arg(long)]
    #[serde(default)]
    pub(crate) exclude: Vec<String>,

    /// Deprecated; use `--exclude-path` instead
    #[arg(long)]
    #[serde(default)]
    pub(crate) exclude_file: Vec<String>,

    /// Exclude file path from getting checked.
    #[arg(long)]
    #[serde(default)]
    pub(crate) exclude_path: Vec<PathBuf>,

    /// Exclude all private IPs from checking.
    /// Equivalent to `--exclude-private --exclude-link-local --exclude-loopback`
    #[arg(short = 'E', long, verbatim_doc_comment)]
    #[serde(default)]
    pub(crate) exclude_all_private: bool,

    /// Exclude private IP address ranges from checking
    #[arg(long)]
    #[serde(default)]
    pub(crate) exclude_private: bool,

    /// Exclude link-local IP address range from checking
    #[arg(long)]
    #[serde(default)]
    pub(crate) exclude_link_local: bool,

    /// Exclude loopback IP address range and localhost from checking
    #[arg(long)]
    #[serde(default)]
    pub(crate) exclude_loopback: bool,

    /// Exclude all mail addresses from checking
    /// (deprecated; excluded by default)
    #[arg(long)]
    #[serde(default)]
    pub(crate) exclude_mail: bool,

    /// Also check email addresses
    #[arg(long)]
    #[serde(default)]
    pub(crate) include_mail: bool,

    /// Remap URI matching pattern to different URI
    #[serde(default)]
    #[arg(long)]
    pub(crate) remap: Vec<String>,

    /// Automatically append file extensions to `file://` URIs as needed
    #[serde(default)]
    #[arg(
        long,
        value_delimiter = ',',
        long_help = "Test the specified file extensions for URIs when checking files locally.
Multiple extensions can be separated by commas. Extensions will be checked in
order of appearance.

Example: --fallback-extensions html,htm,php,asp,aspx,jsp,cgi"
    )]
    pub(crate) fallback_extensions: Vec<String>,

    /// Custom request header
    #[arg(long)]
    #[serde(default)]
    pub(crate) header: Vec<String>,

    /// A List of accepted status codes for valid links
    #[arg(
        short,
        long,
        default_value_t,
        long_help = "A List of accepted status codes for valid links

The following accept range syntax is supported: [start]..[=]end|code. Some valid
examples are:

- 200..=204
- 200..204
- ..=204
- ..204
- 200

Use \"lychee --accept '200..=204, 429, 500' <inputs>...\" to provide a comma-
separated list of accepted status codes. This example will accept 200, 201,
202, 203, 204, 429, and 500 as valid status codes."
    )]
    #[serde(default = "accept_selector")]
    pub(crate) accept: StatusCodeSelector,

    /// Enable the checking of fragments in links.
    #[arg(long)]
    #[serde(default)]
    pub(crate) include_fragments: bool,

    /// Website timeout in seconds from connect to response finished
    #[arg(short, long, default_value = &TIMEOUT_STR)]
    #[serde(default = "timeout")]
    pub(crate) timeout: usize,

    /// Minimum wait time in seconds between retries of failed requests
    #[arg(short, long, default_value = &RETRY_WAIT_TIME_STR)]
    #[serde(default = "retry_wait_time")]
    pub(crate) retry_wait_time: usize,

    /// Request method
    // Using `-X` as a short param similar to curl
    #[arg(short = 'X', long, default_value = DEFAULT_METHOD)]
    #[serde(default = "method")]
    pub(crate) method: String,

    /// Base URL or website root directory to check relative URLs
    /// e.g. <https://example.com> or `/path/to/public`
    #[arg(short, long, value_parser= parse_base)]
    #[serde(default)]
    pub(crate) base: Option<Base>,

    /// Root path to use when checking absolute local links,
    /// must be an absolute path
    #[arg(long)]
    #[serde(default)]
    pub(crate) root_dir: Option<PathBuf>,

    /// Basic authentication support. E.g. `http://example.com username:password`
    #[arg(long)]
    #[serde(default)]
    pub(crate) basic_auth: Option<Vec<BasicAuthSelector>>,

    /// GitHub API token to use when checking github.com links, to avoid rate limiting
    #[arg(long, env = "GITHUB_TOKEN", hide_env_values = true)]
    #[serde(default)]
    pub(crate) github_token: Option<SecretString>,

    /// Skip missing input files (default is to error if they don't exist)
    #[arg(long)]
    #[serde(default)]
    pub(crate) skip_missing: bool,

    /// Do not skip files that would otherwise be ignored by
    /// '.gitignore', '.ignore', or the global ignore file.
    #[arg(long)]
    #[serde(default)]
    pub(crate) no_ignore: bool,

    /// Do not skip hidden directories and files.
    #[arg(long)]
    #[serde(default)]
    pub(crate) hidden: bool,

    /// Find links in verbatim sections like `pre`- and `code` blocks
    #[arg(long)]
    #[serde(default)]
    pub(crate) include_verbatim: bool,

    /// Ignore case when expanding filesystem path glob inputs
    #[arg(long)]
    #[serde(default)]
    pub(crate) glob_ignore_case: bool,

    /// Output file of status report
    #[arg(short, long, value_parser)]
    #[serde(default)]
    pub(crate) output: Option<PathBuf>,

    /// Set the output display mode. Determines how results are presented in the terminal
    #[arg(long, default_value = "color", value_parser = PossibleValuesParser::new(OutputMode::VARIANTS).map(|s| s.parse::<OutputMode>().unwrap()))]
    #[serde(default)]
    pub(crate) mode: OutputMode,

    /// Output format of final status report
    #[arg(short, long, default_value = "compact", value_parser = PossibleValuesParser::new(StatsFormat::VARIANTS).map(|s| s.parse::<StatsFormat>().unwrap()))]
    #[serde(default)]
    pub(crate) format: StatsFormat,

    /// When HTTPS is available, treat HTTP links as errors
    #[arg(long)]
    #[serde(default)]
    pub(crate) require_https: bool,

    /// Tell lychee to read cookies from the given file.
    /// Cookies will be stored in the cookie jar and sent with requests.
    /// New cookies will be stored in the cookie jar and existing cookies will be updated.
    #[arg(long)]
    #[serde(default)]
    pub(crate) cookie_jar: Option<PathBuf>,
}

impl Config {
    /// Load configuration from a file
    pub(crate) fn load_from_file(path: &Path) -> Result<Config> {
        // Read configuration file
        let contents = fs::read_to_string(path)?;
        toml::from_str(&contents).with_context(|| "Failed to parse configuration file")
    }

    /// Merge the configuration from TOML into the CLI configuration
    pub(crate) fn merge(&mut self, toml: Config) {
        fold_in! {
            // Destination and source configs
            self, toml;

            // Keys with defaults to assign
            verbose: Verbosity::default();
            cache: false;
            no_progress: false;
            max_redirects: DEFAULT_MAX_REDIRECTS;
            max_retries: DEFAULT_MAX_RETRIES;
            max_concurrency: DEFAULT_MAX_CONCURRENCY;
            max_cache_age: humantime::parse_duration(DEFAULT_MAX_CACHE_AGE).unwrap();
            cache_exclude_status: StatusCodeExcluder::default();
            threads: None;
            user_agent: DEFAULT_USER_AGENT;
            insecure: false;
            scheme: Vec::<String>::new();
            include: Vec::<String>::new();
            exclude: Vec::<String>::new();
            exclude_file: Vec::<String>::new(); // deprecated
            exclude_path: Vec::<PathBuf>::new();
            exclude_all_private: false;
            exclude_private: false;
            exclude_link_local: false;
            exclude_loopback: false;
            exclude_mail: false;
            format: StatsFormat::default();
            remap: Vec::<String>::new();
            fallback_extensions: Vec::<String>::new();
            header: Vec::<String>::new();
            timeout: DEFAULT_TIMEOUT_SECS;
            retry_wait_time: DEFAULT_RETRY_WAIT_TIME_SECS;
            method: DEFAULT_METHOD;
            base: None;
            basic_auth: None;
            skip_missing: false;
            include_verbatim: false;
            include_mail: false;
            glob_ignore_case: false;
            output: None;
            require_https: false;
            cookie_jar: None;
            include_fragments: false;
            accept: StatusCodeSelector::default();
        }

        if self
            .github_token
            .as_ref()
            .map(ExposeSecret::expose_secret)
            .is_none()
            && toml
                .github_token
                .as_ref()
                .map(ExposeSecret::expose_secret)
                .is_some()
        {
            self.github_token = toml.github_token;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accept_status_codes() {
        let toml = Config {
            accept: StatusCodeSelector::from_str("200..=204, 429, 500").unwrap(),
            ..Default::default()
        };

        let mut cli = Config::default();
        cli.merge(toml);

        assert!(cli.accept.contains(429));
        assert!(cli.accept.contains(200));
        assert!(cli.accept.contains(203));
        assert!(cli.accept.contains(204));
        assert!(!cli.accept.contains(205));
    }

    #[test]
    fn test_default() {
        let cli = Config::default();

        assert_eq!(
            cli.accept,
            StatusCodeSelector::from_str("100..=103,200..=299").expect("no error")
        );
        assert_eq!(cli.cache_exclude_status, StatusCodeExcluder::new());
    }
}
