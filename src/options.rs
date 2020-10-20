use anyhow::{Error, Result};
use serde::Deserialize;
use std::{fs, io::ErrorKind};
use structopt::StructOpt;

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
    ( $( $cli:expr , $toml:expr , $default:expr; )* ) => {
        $(
            if $cli == $default && $toml != $default {
                $cli = $toml;
            }
        )*
    };
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "lychee",
    about = "A boring link checker for my projects (and maybe yours)"
)]
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
pub(crate) struct Config {
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

    /// Number of threads to utilize.
    /// Defaults to number of cores available to the system
    #[structopt(short = "T", long)]
    #[serde(default)]
    pub threads: Option<usize>,

    /// User agent
    #[structopt(short, long, default_value = "curl/7.71.1")]
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

    /// Exclude URLs from checking (supports regex)
    #[structopt(short, long)]
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
    #[structopt(short, long, default_value = "20")]
    #[serde(default = "timeout")]
    pub timeout: String,

    /// Request method
    #[structopt(short = "M", long, default_value = "get")]
    #[serde(default = "get")]
    pub method: String,
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
    pub(crate) fn merge(&mut self, toml: Config) {
        fold_in! {
            self.verbose, toml.verbose, false;
            self.progress, toml.progress, false;
            self.max_redirects, toml.max_redirects, 10;
            self.threads, toml.threads, None;
            self.user_agent, toml.user_agent, "curl/7.71.1";
            self.insecure, toml.insecure, false;
            self.scheme, toml.scheme, None;
            self.exclude, toml.exclude, Vec::<String>::new();
            self.exclude_all_private, toml.exclude_all_private, false;
            self.exclude_private, toml.exclude_private, false;
            self.exclude_link_local, toml.exclude_link_local, false;
            self.exclude_loopback, toml.exclude_loopback, false;
            self.headers, toml.headers, Vec::<String>::new();
            self.accept, toml.accept, None;
            self.timeout, toml.timeout, "20";
            self.method, toml.method, "get";
        }
    }
}

// Generate the functions for serde defaults
default_function! {
    user_agent: String = "curl/7.71.1".to_string();
    timeout: String = "20".to_string();
    get: String = "get".to_string();
}
