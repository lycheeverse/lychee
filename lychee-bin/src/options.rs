use crate::parse::{parse_base, parse_statuscodes};
use anyhow::{anyhow, Context, Error, Result};
use clap::{arg, Parser};
use const_format::{concatcp, formatcp};
use lychee_lib::{
    Base, Input, InputSource, PathExcludes, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_RETRIES,
    DEFAULT_RETRY_WAIT_TIME_SECS, DEFAULT_TIMEOUT_SECS, DEFAULT_USER_AGENT,
};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::{
    collections::HashSet, env, fs, io::ErrorKind, path::PathBuf, str::FromStr, time::Duration,
};

pub(crate) const LYCHEE_IGNORE_FILE: &str = ".lycheeignore";
pub(crate) const LYCHEE_CACHE_FILE: &str = ".lycheecache";

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
const TIMEOUT_STR: &str = concatcp!(DEFAULT_TIMEOUT_SECS);
const RETRY_WAIT_TIME_STR: &str = concatcp!(DEFAULT_RETRY_WAIT_TIME_SECS);

#[derive(Debug, Deserialize, Clone)]
pub(crate) enum Format {
    Compact,
    Detailed,
    Json,
    Markdown,
    Raw,
}

impl FromStr for Format {
    type Err = Error;
    fn from_str(format: &str) -> Result<Self, Self::Err> {
        match format.to_lowercase().as_str() {
            "compact" | "string" => Ok(Format::Compact),
            "detailed" => Ok(Format::Detailed),
            "json" => Ok(Format::Json),
            "markdown" | "md" => Ok(Format::Markdown),
            "raw" => Ok(Format::Raw),
            _ => Err(anyhow!("Unknown format {}", format)),
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
    max_concurrency: usize = DEFAULT_MAX_CONCURRENCY;
    max_cache_age: Duration = humantime::parse_duration(DEFAULT_MAX_CACHE_AGE).unwrap();
    user_agent: String = DEFAULT_USER_AGENT.to_string();
    timeout: usize = DEFAULT_TIMEOUT_SECS;
    retry_wait_time: usize = DEFAULT_RETRY_WAIT_TIME_SECS;
    method: String = DEFAULT_METHOD.to_string();
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

#[derive(Parser, Debug)]
#[command(version, about)]
/// A fast, async link checker
///
/// Finds broken URLs and mail addresses inside Markdown, HTML,
/// `reStructuredText`, websites and more!
pub(crate) struct LycheeOptions {
    /// The inputs (where to get links to check from).
    /// These can be: files (e.g. `README.md`), file globs (e.g. `"~/git/*/README.md"`),
    /// remote URLs (e.g. `https://example.com/README.md`) or standard input (`-`).
    /// NOTE: Use `--` to separate inputs from options that allow multiple arguments.
    #[arg(name = "inputs", required = true)]
    raw_inputs: Vec<String>,

    /// Configuration file to use
    #[arg(short, long = "config", default_value = "./lychee.toml")]
    pub(crate) config_file: String,

    #[command(flatten)]
    pub(crate) config: Config,
}

impl LycheeOptions {
    /// Get parsed inputs from options.
    // This depends on the config, which is why a method is required (we could
    // accept a `Vec<Input>` in `LycheeOptions` and do the conversion there, but
    // we wouldn't get access to `glob_ignore_case`.
    pub(crate) fn inputs(&self) -> Result<Vec<Input>> {
        self.raw_inputs
            .iter()
            .map(|s| {
                let source = InputSource::new(s, self.config.glob_ignore_case)?;
                if let InputSource::FsPath(ref src) = source {
                    let root = src.parent().unwrap().to_path_buf();
                    println!("root: {:?}", root);
                    // If root is empty, use the current directory
                    let root = if root == PathBuf::from("") {
                        env::current_dir()
                    } else {
                        Ok(root)
                    }?;
                    println!("Final root: {:?}", root);

                    let path_excludes = PathExcludes::new(
                        root,
                        self.config.exclude_path.clone(),
                        !self.config.no_ignore,
                    )?;
                    Input::new(source, None, Some(path_excludes))
                } else if source.is_glob() {
                    let path_excludes = PathExcludes::new(
                        env::current_dir()?,
                        self.config.exclude_path.clone(),
                        !self.config.no_ignore,
                    )?;
                    Input::new(source, None, Some(path_excludes))
                } else {
                    Input::new(source, None, None)
                }
            })
            .collect::<Result<_, _>>()
            .context("Cannot parse inputs from arguments")
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Parser, Debug, Deserialize, Clone)]
pub(crate) struct Config {
    /// Verbose program output
    #[arg(short, long)]
    #[serde(default)]
    pub(crate) verbose: bool,

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

    /// Don't perform any link checking.
    /// Instead, dump all the links extracted from inputs that would be checked
    #[arg(long)]
    #[serde(default)]
    pub(crate) dump: bool,

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

    /// Only test links with the given schemes (e.g. http and https)
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
    #[arg(long)]
    #[serde(default)]
    pub(crate) exclude_mail: bool,

    /// Remap URI matching pattern to different URI
    #[serde(default)]
    #[arg(long)]
    pub(crate) remap: Vec<String>,

    /// Custom request header
    #[arg(long)]
    #[serde(default)]
    pub(crate) header: Vec<String>,

    /// Comma-separated list of accepted status codes for valid links
    #[arg(short, long, value_parser = parse_statuscodes)]
    #[serde(default)]
    pub(crate) accept: Option<HashSet<u16>>,

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
    /// e.g. https://example.com or `/path/to/public`
    #[arg(short, long, value_parser= parse_base)]
    #[serde(default)]
    pub(crate) base: Option<Base>,

    /// Basic authentication support. E.g. `username:password`
    #[arg(long)]
    #[serde(default)]
    pub(crate) basic_auth: Option<String>,

    /// GitHub API token to use when checking github.com links, to avoid rate limiting
    #[arg(long, env = "GITHUB_TOKEN", hide_env_values = true)]
    #[serde(default)]
    pub(crate) github_token: Option<SecretString>,

    /// Skip missing input files (default is to error if they don't exist)
    #[arg(long)]
    #[serde(default)]
    pub(crate) skip_missing: bool,

    /// Find links in verbatim sections like `pre`- and `code` blocks
    #[arg(long)]
    #[serde(default)]
    pub(crate) include_verbatim: bool,

    /// Ignore case when expanding filesystem path glob inputs
    #[arg(long)]
    #[serde(default)]
    pub(crate) glob_ignore_case: bool,

    /// Don't respect ignore files (`.gitignore`)
    #[arg(long)]
    #[serde(default)]
    pub(crate) no_ignore: bool,

    /// Output file of status report
    #[arg(short, long, value_parser)]
    #[serde(default)]
    pub(crate) output: Option<PathBuf>,

    /// Output format of final status report (compact, detailed, json, markdown)
    #[arg(short, long, default_value = "compact")]
    #[serde(default)]
    pub(crate) format: Format,

    /// When HTTPS is available, treat HTTP links as errors
    #[arg(long)]
    #[serde(default)]
    pub(crate) require_https: bool,
}

impl Config {
    /// Load configuration from a file
    pub(crate) fn load_from_file(path: &str) -> Result<Option<Config>> {
        // Read configuration file
        let result = fs::read(path);

        // Ignore a file-not-found error
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
            max_concurrency: DEFAULT_MAX_CONCURRENCY;
            max_cache_age: humantime::parse_duration(DEFAULT_MAX_CACHE_AGE).unwrap();
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
            remap: Vec::<String>::new();
            header: Vec::<String>::new();
            accept: None;
            timeout: DEFAULT_TIMEOUT_SECS;
            retry_wait_time: DEFAULT_RETRY_WAIT_TIME_SECS;
            method: DEFAULT_METHOD;
            base: None;
            basic_auth: None;
            skip_missing: false;
            include_verbatim: false;
            glob_ignore_case: false;
            output: None;
            require_https: false;
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
