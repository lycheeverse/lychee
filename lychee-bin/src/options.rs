use std::{convert::TryFrom, fs, io::ErrorKind, path::PathBuf, str::FromStr};

use anyhow::{anyhow, Error, Result};
use lazy_static::lazy_static;
use lychee_lib::{
    Base, Input, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_RETRIES, DEFAULT_TIMEOUT, DEFAULT_USER_AGENT,
};
use serde::Deserialize;
use std::time::Duration;
use structopt::StructOpt;

pub(crate) const LYCHEE_IGNORE_FILE: &str = ".lycheeignore";
pub(crate) const LYCHEE_CACHE_FILE: &str = ".lycheecache";

const METHOD: &str = "get";
const MAX_CONCURRENCY: usize = 128;

// this exists because structopt requires `&str` type values for defaults
// (we can't use e.g. `TIMEOUT` or `timeout()` which gets created for serde)
lazy_static! {
    static ref MAX_CONCURRENCY_STR: String = MAX_CONCURRENCY.to_string();
    static ref MAX_REDIRECTS_STR: String = DEFAULT_MAX_REDIRECTS.to_string();
    static ref MAX_RETRIES_STR: String = DEFAULT_MAX_RETRIES.to_string();
    static ref STRUCTOPT_HELP_MSG_CACHE: String = format!(
        "Use request cache stored on disk at `{}`",
        LYCHEE_CACHE_FILE
    );
    static ref STRUCTOPT_HELP_MSG_IGNORE_FILE: String = format!(
        "File or files that contain URLs to be excluded from checking. Regular
expressions supported; one pattern per line. Automatically excludes
patterns from `{}` if file exists",
        LYCHEE_IGNORE_FILE
    );
    static ref TIMEOUT_STR: String = DEFAULT_TIMEOUT.to_string();
}

#[derive(Clone, Debug, Deserialize, Copy)]
pub(crate) enum Format {
    Compact,
    Detailed,
    Json,
    Markdown,
}

impl FromStr for Format {
    type Err = Error;
    fn from_str(format: &str) -> Result<Self, Self::Err> {
        match format {
            "compact" | "string" => Ok(Format::Compact),
            "detailed" => Ok(Format::Detailed),
            "json" => Ok(Format::Json),
            "markdown" | "md" => Ok(Format::Markdown),
            _ => Err(anyhow!("Could not parse format {}", format)),
        }
    }
}

impl Default for Format {
    fn default() -> Self {
        Format::Compact
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
    max_concurrency: usize = MAX_CONCURRENCY;
    user_agent: String = DEFAULT_USER_AGENT.to_string();
    timeout: usize = DEFAULT_TIMEOUT;
    method: String = METHOD.to_string();
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

fn parse_base(src: &str) -> Result<Base, lychee_lib::ErrorKind> {
    Base::try_from(src)
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "lychee",
    about = "A glorious link checker.\n\nProject home page: https://github.com/lycheeverse/lychee"
)]
pub(crate) struct LycheeOptions {
    /// The inputs (where to get links to check from).
    /// These can be: files (e.g. `README.md`), file globs (e.g. `"~/git/*/README.md"`),
    /// remote URLs (e.g. `https://example.org/README.md`) or standard input (`-`).
    /// NOTE: Use `--` to separate inputs from options that allow multiple arguments.
    #[structopt(name = "inputs", required = true)]
    raw_inputs: Vec<String>,

    /// Configuration file to use
    #[structopt(short, long = "config", default_value = "./lychee.toml")]
    pub(crate) config_file: String,

    #[structopt(flatten)]
    pub(crate) config: Config,
}

impl LycheeOptions {
    // This depends on the config, which is why a method is required (we could
    // accept a `Vec<Input>` in `LycheeOptions` and do the conversion there,
    // but we'd get no access to `glob_ignore_case`.
    /// Get parsed inputs from options.
    pub(crate) fn inputs(&self) -> Vec<Input> {
        self.raw_inputs
            .iter()
            .map(|s| Input::new(s, None, self.config.glob_ignore_case))
            .collect()
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Deserialize, StructOpt)]
pub(crate) struct Config {
    /// Verbose program output
    #[structopt(short, long)]
    #[serde(default)]
    pub(crate) verbose: bool,

    /// Do not show progress bar.
    /// This is recommended for non-interactive shells (e.g. for continuous integration)
    #[structopt(short, long, verbatim_doc_comment)]
    #[serde(default)]
    pub(crate) no_progress: bool,

    #[structopt(help = &STRUCTOPT_HELP_MSG_CACHE)]
    #[structopt(long)]
    #[serde(default)]
    pub(crate) cache: bool,

    /// Discard all cached requests older than this duration
    #[structopt(
        long,
        parse(try_from_str = humantime::parse_duration),
        default_value = "1d"
    )]
    pub(crate) max_cache_age: Duration,

    /// Don't perform any link checking.
    /// Instead, dump all the links extracted from inputs that would be checked
    #[structopt(long)]
    #[serde(default)]
    pub(crate) dump: bool,

    /// Maximum number of allowed redirects
    #[structopt(short, long, default_value = &MAX_REDIRECTS_STR)]
    #[serde(default = "max_redirects")]
    pub(crate) max_redirects: usize,

    /// Maximum number of retries per request
    #[structopt(long, default_value = &MAX_RETRIES_STR)]
    #[serde(default = "max_retries")]
    pub(crate) max_retries: u64,

    /// Maximum number of concurrent network requests
    #[structopt(long, default_value = &MAX_CONCURRENCY_STR)]
    #[serde(default = "max_concurrency")]
    pub(crate) max_concurrency: usize,

    /// Number of threads to utilize.
    /// Defaults to number of cores available to the system
    #[structopt(short = "T", long)]
    #[serde(default)]
    pub(crate) threads: Option<usize>,

    /// User agent
    #[structopt(short, long, default_value = DEFAULT_USER_AGENT)]
    #[serde(default = "user_agent")]
    pub(crate) user_agent: String,

    /// Proceed for server connections considered insecure (invalid TLS)
    #[structopt(short, long)]
    #[serde(default)]
    pub(crate) insecure: bool,

    /// Only test links with the given schemes (e.g. http and https)
    #[structopt(short, long)]
    #[serde(default)]
    pub(crate) scheme: Vec<String>,

    /// Only check local files and block network requests.
    #[structopt(long)]
    #[serde(default)]
    pub(crate) offline: bool,

    /// URLs to check (supports regex). Has preference over all excludes.
    #[structopt(long)]
    #[serde(default)]
    pub(crate) include: Vec<String>,

    /// Exclude URLs from checking (supports regex)
    #[structopt(long)]
    #[serde(default)]
    pub(crate) exclude: Vec<String>,

    #[structopt(help = &STRUCTOPT_HELP_MSG_IGNORE_FILE)]
    #[structopt(long)]
    #[serde(default)]
    pub(crate) exclude_file: Vec<String>,

    /// Exclude all private IPs from checking.
    /// Equivalent to `--exclude-private --exclude-link-local --exclude-loopback`
    #[structopt(short = "E", long, verbatim_doc_comment)]
    #[serde(default)]
    pub(crate) exclude_all_private: bool,

    /// Exclude private IP address ranges from checking
    #[structopt(long)]
    #[serde(default)]
    pub(crate) exclude_private: bool,

    /// Exclude link-local IP address range from checking
    #[structopt(long)]
    #[serde(default)]
    pub(crate) exclude_link_local: bool,

    /// Exclude loopback IP address range and localhost from checking
    #[structopt(long)]
    #[serde(default)]
    pub(crate) exclude_loopback: bool,

    /// Exclude all mail addresses from checking
    #[structopt(long)]
    #[serde(default)]
    pub(crate) exclude_mail: bool,

    /// Custom request headers
    #[structopt(short, long)]
    #[serde(default)]
    pub(crate) headers: Vec<String>,

    /// Comma-separated list of accepted status codes for valid links
    #[structopt(short, long)]
    #[serde(default)]
    pub(crate) accept: Option<String>,

    /// Website timeout from connect to response finished
    #[structopt(short, long, default_value = &TIMEOUT_STR)]
    #[serde(default = "timeout")]
    pub(crate) timeout: usize,

    /// Request method
    // Using `-X` as a short param similar to curl
    #[structopt(short = "X", long, default_value = METHOD)]
    #[serde(default = "method")]
    pub(crate) method: String,

    /// Base URL or website root directory to check relative URLs
    /// e.g. https://example.org or `/path/to/public`
    #[structopt(short, long, parse(try_from_str = parse_base))]
    #[serde(default)]
    pub(crate) base: Option<Base>,

    /// Basic authentication support. E.g. `username:password`
    #[structopt(long)]
    #[serde(default)]
    pub(crate) basic_auth: Option<String>,

    /// GitHub API token to use when checking github.com links, to avoid rate limiting
    #[structopt(long, env = "GITHUB_TOKEN", hide_env_values = true)]
    #[serde(default)]
    pub(crate) github_token: Option<String>,

    /// Skip missing input files (default is to error if they don't exist)
    #[structopt(long)]
    #[serde(default)]
    pub(crate) skip_missing: bool,

    /// Ignore case when expanding filesystem path glob inputs
    #[structopt(long)]
    #[serde(default)]
    pub(crate) glob_ignore_case: bool,

    /// Output file of status report
    #[structopt(short, long, parse(from_os_str))]
    #[serde(default)]
    pub(crate) output: Option<PathBuf>,

    /// Output format of final status report (compact, detailed, json, markdown)
    #[structopt(short, long, default_value = "compact")]
    #[serde(default)]
    pub(crate) format: Format,

    /// When HTTPS is available, treat HTTP links as errors
    #[structopt(long)]
    #[serde(default)]
    pub(crate) require_https: bool,
}

impl Config {
    /// Load configuration from a file
    pub(crate) fn load_from_file(path: &str) -> Result<Option<Config>> {
        // Read configuration file
        let result = fs::read(path);

        // Ignore a file not found error
        let contents = match result {
            Ok(c) => c,
            Err(e) => {
                return match e.kind() {
                    ErrorKind::NotFound => Ok(None),
                    _ => Err(Error::from(e)),
                }
            }
        };

        Ok(Some(toml::from_slice(&contents)?))
    }

    /// Merge the configuration from TOML into the CLI configuration
    pub(crate) fn merge(&mut self, toml: Config) {
        fold_in! {
            // Destination and source configs
            self, toml;

            // Keys with defaults to assign
            verbose: false;
            cache: false;
            no_progress: false;
            max_redirects: DEFAULT_MAX_REDIRECTS;
            max_retries: DEFAULT_MAX_RETRIES;
            max_concurrency: MAX_CONCURRENCY;
            threads: None;
            user_agent: DEFAULT_USER_AGENT;
            insecure: false;
            scheme: Vec::<String>::new();
            include: Vec::<String>::new();
            exclude: Vec::<String>::new();
            exclude_file: Vec::<String>::new();
            exclude_all_private: false;
            exclude_private: false;
            exclude_link_local: false;
            exclude_loopback: false;
            exclude_mail: false;
            headers: Vec::<String>::new();
            accept: None;
            timeout: DEFAULT_TIMEOUT;
            method: METHOD;
            base: None;
            basic_auth: None;
            github_token: None;
            skip_missing: false;
            glob_ignore_case: false;
            output: None;
            require_https: false;
        }
    }
}
