use anyhow::{Error, Result};
use serde::Deserialize;
use std::{fs, io::ErrorKind};
use structopt::{clap::crate_version, StructOpt};

pub(crate) const USER_AGENT: &str = concat!("lychee/", crate_version!());
const METHOD: &str = "get";
const TIMEOUT: &str = "20";
const MAX_CONCURRENCY: &str = "128";

// Macro for generating default functions to be used by serde
macro_rules! default_function {
    ( $( $name:ident : $T:ty = $e:expr; )* ) => {
        $(
            fn $name() -> $T {
                $e
            }
        )*
    };
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
#[structopt(name = "lychee", about = "A glorious link checker")]
pub(crate) struct LycheeOptions {
    /// Input files
    pub inputs: Vec<String>,

    /// Configuration file to use
    #[structopt(short, long = "config", default_value = "./lychee.toml")]
    pub config_file: String,

    #[structopt(flatten)]
    pub config: Config,
}

#[derive(Debug, Deserialize, StructOpt)]
pub struct Config {
    /// Verbose program output
    #[structopt(short, long)]
    #[serde(default)]
    pub verbose: bool,

    /// Show progress
    #[structopt(short, long)]
    #[serde(default)]
    pub progress: bool,

    /// Maximum number of allowed redirects
    #[structopt(short, long, default_value = "10")]
    #[serde(default)]
    pub max_redirects: usize,

    /// Maximum number of concurrent network requests
    #[structopt(long, default_value = MAX_CONCURRENCY)]
    #[serde(default)]
    pub max_concurrency: String,

    /// Number of threads to utilize.
    /// Defaults to number of cores available to the system
    #[structopt(short = "T", long)]
    #[serde(default)]
    pub threads: Option<usize>,

    /// User agent
    #[structopt(short, long, default_value = USER_AGENT)]
    #[serde(default = "user_agent")]
    pub user_agent: String,

    /// Proceed for server connections considered insecure (invalid TLS)
    #[structopt(short, long)]
    #[serde(default)]
    pub insecure: bool,

    /// Only test links with the given scheme (e.g. https)
    #[structopt(short, long)]
    #[serde(default)]
    pub scheme: Option<String>,

    /// URLs to check (supports regex). Has preference over all excludes.
    #[structopt(long)]
    #[serde(default)]
    pub include: Vec<String>,

    /// Exclude URLs from checking (supports regex)
    #[structopt(long)]
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Exclude all private IPs from checking.
    /// Equivalent to `--exclude-private --exclude-link-local --exclude-loopback`
    #[structopt(short = "E", long)]
    #[serde(default)]
    pub exclude_all_private: bool,

    /// Exclude private IP address ranges from checking
    #[structopt(long)]
    #[serde(default)]
    pub exclude_private: bool,

    /// Exclude link-local IP address range from checking
    #[structopt(long)]
    #[serde(default)]
    pub exclude_link_local: bool,

    /// Exclude loopback IP address range from checking
    #[structopt(long)]
    #[serde(default)]
    pub exclude_loopback: bool,

    /// Custom request headers
    #[structopt(short, long)]
    #[serde(default)]
    pub headers: Vec<String>,

    /// Comma-separated list of accepted status codes for valid links
    #[structopt(short, long)]
    #[serde(default)]
    pub accept: Option<String>,

    /// Website timeout from connect to response finished
    #[structopt(short, long, default_value = TIMEOUT)]
    #[serde(default = "timeout")]
    pub timeout: String,

    /// Request method
    // Using `-X` as a short param similar to curl
    #[structopt(short = "X", long, default_value = METHOD)]
    #[serde(default = "method")]
    pub method: String,

    #[structopt(short, long, help = "Base URL to check relative URls")]
    #[serde(default)]
    pub base_url: Option<String>,

    #[structopt(long, help = "Basic authentication support. Ex 'username:password'")]
    #[serde(default)]
    pub basic_auth: Option<String>,

    #[structopt(
        long,
        help = "GitHub API token to use when checking github.com links, to avoid rate limiting",
        env = "GITHUB_TOKEN"
    )]
    #[serde(default)]
    pub github_token: Option<String>,
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
                    ErrorKind::NotFound => {
                        println!("[WARN] could not find configuration file, using arguments");
                        Ok(None)
                    }
                    _ => Err(Error::from(e)),
                }
            }
        };

        Ok(Some(toml::from_slice(&contents)?))
    }

    /// Merge the configuration from TOML into the CLI configuration
    pub(crate) fn merge(mut self, toml: Config) -> Config {
        fold_in! {
            // Destination and source configs
            self, toml;

            // Keys with defaults to assign
            verbose: false;
            progress: false;
            max_redirects: 10;
            max_concurrency: MAX_CONCURRENCY;
            threads: None;
            user_agent: USER_AGENT;
            insecure: false;
            scheme: None;
            include: Vec::<String>::new();
            exclude: Vec::<String>::new();
            exclude_all_private: false;
            exclude_private: false;
            exclude_link_local: false;
            exclude_loopback: false;
            headers: Vec::<String>::new();
            accept: None;
            timeout: TIMEOUT;
            method: METHOD;
            base_url: None;
            basic_auth: None;
            github_token: None;
        }

        self
    }
}

// Generate the functions for serde defaults
default_function! {
    user_agent: String = USER_AGENT.to_string();
    timeout: String = TIMEOUT.to_string();
    method: String = METHOD.to_string();
}
