use std::{fs, io::ErrorKind, path::PathBuf, str::FromStr};

use anyhow::{anyhow, Error, Result};
use lazy_static::lazy_static;
use lychee_lib::collector::Input;
use reqwest::Url;
use serde::Deserialize;
use structopt::{clap::crate_version, StructOpt};

const METHOD: &str = "get";
const TIMEOUT: usize = 20;
const MAX_CONCURRENCY: usize = 128;
const MAX_REDIRECTS: usize = 10;
const USER_AGENT: &str = concat!("lychee/", crate_version!());

// this exists because structopt requires `&str` type values for defaults
// (we can't use e.g. `TIMEOUT` or `timeout()` which gets created for serde)
lazy_static! {
    static ref TIMEOUT_STR: String = TIMEOUT.to_string();
    static ref MAX_CONCURRENCY_STR: String = MAX_CONCURRENCY.to_string();
    static ref MAX_REDIRECTS_STR: String = MAX_REDIRECTS.to_string();
}

#[derive(Debug, Deserialize)]
pub(crate) enum Format {
    String,
    Json,
}

impl FromStr for Format {
    type Err = Error;
    fn from_str(format: &str) -> Result<Self, Self::Err> {
        match format {
            "string" => Ok(Format::String),
            "json" => Ok(Format::Json),
            _ => Err(anyhow!("Could not parse format {}", format)),
        }
    }
}

impl Default for Format {
    fn default() -> Self {
        Format::String
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
    max_redirects: usize = MAX_REDIRECTS;
    max_concurrency: usize = MAX_CONCURRENCY;
    user_agent: String = USER_AGENT.to_string();
    timeout: usize = TIMEOUT;
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

#[derive(Debug, StructOpt)]
#[structopt(
    name = "lychee",
    about = "A glorious link checker.\n\nProject home page: https://github.com/lycheeverse/lychee"
)]
pub(crate) struct LycheeOptions {
    /// The inputs (where to get links to check from).
    /// These can be: files (e.g. `README.md`), file globs (e.g. `"~/git/*/README.md"`),
    /// remote URLs (e.g. `https://example.org/README.md`) or standard input (`-`).
    /// Prefix with `--` to separate inputs from options that allow multiple arguments.
    #[structopt(name = "inputs", default_value = "README.md")]
    raw_inputs: Vec<String>,

    /// Configuration file to use
    #[structopt(short, long = "config", default_value = "./lychee.toml")]
    pub(crate) config_file: String,

    #[structopt(flatten)]
    pub(crate) config: Config,
}

impl LycheeOptions {
    // This depends on config, which is why a method is required (we could
    // accept a `Vec<Input>` in `LycheeOptions` and do the conversion there,
    // but we'd get no access to `glob_ignore_case`.
    /// Get parsed inputs from options.
    pub(crate) fn inputs(&self) -> Vec<Input> {
        self.raw_inputs
            .iter()
            .map(|s| Input::new(s, self.config.glob_ignore_case))
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

    /// Don't perform any link checking.
    /// Instead, dump all the links extracted from inputs that would be checked
    #[structopt(long)]
    #[serde(default)]
    pub(crate) no_check: bool,

    /// Maximum number of allowed redirects
    #[structopt(short, long, default_value = &MAX_REDIRECTS_STR)]
    #[serde(default = "max_redirects")]
    pub(crate) max_redirects: usize,

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
    #[structopt(short, long, default_value = USER_AGENT)]
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

    /// URLs to check (supports regex). Has preference over all excludes.
    #[structopt(long)]
    #[serde(default)]
    pub(crate) include: Vec<String>,

    /// Exclude URLs from checking (supports regex)
    #[structopt(long)]
    #[serde(default)]
    pub(crate) exclude: Vec<String>,

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

    /// Exclude loopback IP address range from checking
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

    /// Base URL to check relative URLs
    #[structopt(short, long, parse(try_from_str))]
    #[serde(default)]
    pub(crate) base_url: Option<Url>,

    /// Basic authentication support. E.g. `username:password`
    #[structopt(long)]
    #[serde(default)]
    pub(crate) basic_auth: Option<String>,

    /// GitHub API token to use when checking github.com links, to avoid rate limiting
    #[structopt(long, env = "GITHUB_TOKEN")]
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

    /// Output file format of status report (json, string)
    #[structopt(short, long, default_value = "string")]
    #[serde(default)]
    pub(crate) format: Format,
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
            no_progress: false;
            max_redirects: MAX_REDIRECTS;
            max_concurrency: MAX_CONCURRENCY;
            threads: None;
            user_agent: USER_AGENT;
            insecure: false;
            scheme: Vec::<String>::new();
            include: Vec::<String>::new();
            exclude: Vec::<String>::new();
            exclude_all_private: false;
            exclude_private: false;
            exclude_link_local: false;
            exclude_loopback: false;
            exclude_mail: false;
            headers: Vec::<String>::new();
            accept: None;
            timeout: TIMEOUT;
            method: METHOD;
            base_url: None;
            basic_auth: None;
            github_token: None;
            skip_missing: false;
            glob_ignore_case: false;
            output: None;
        }
    }
}
