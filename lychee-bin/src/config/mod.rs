//! Configuration for lychee.
//!
//! This module contains the structs and types for representing the lychee CLI
//! options and the underlying file-based configuration.
//!
//! Submodules define components like header parsing, specific config file
//! loaders (e.g. `lychee.toml`, `Cargo.toml`), and output formatting.

pub(crate) mod header;
pub(crate) mod loaders;
pub(crate) mod output;
pub(crate) mod tls;

pub(crate) use header::*;
pub(crate) use output::*;
pub(crate) use tls::*;

use crate::files_from::FilesFrom;
use crate::generate::GenerateMode;
use crate::parse::parse_base_info;
use crate::verbosity::Verbosity;
use anyhow::{Context, Result, anyhow};
use clap::Parser;
use clap::builder::{PossibleValuesParser, TypedValueParser};
use const_format::formatcp;
use lychee_lib::ratelimit::HostConfigs;
use lychee_lib::{
    BaseInfo, BasicAuthSelector, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_RETRIES,
    DEFAULT_RETRY_WAIT_TIME_SECS, DEFAULT_TIMEOUT_SECS, FileExtensions, FileType, Input,
    StatusCodeSelector, archive::Archive,
};
use lychee_lib::{DEFAULT_USER_AGENT, Preprocessor};
use secrecy::SecretString;
use serde::{Deserialize, Deserializer};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::{path::PathBuf, time::Duration};
use strum::VariantNames;

pub(crate) const LYCHEE_IGNORE_FILE: &str = ".lycheeignore";
pub(crate) const LYCHEE_CACHE_FILE: &str = ".lycheecache";

const HELP_MSG_CACHE: &str = formatcp!("Use request cache stored on disk at `{LYCHEE_CACHE_FILE}`");

// We use a custom help message here because we want to show the default
// value of the config file, but also be able to check if the user has
// provided a custom value. If they didn't, we won't throw an error if
// the file doesn't exist.
const HELP_MSG_CONFIG_FILE: &str = formatcp!(
    "Configuration file to use. Can be specified multiple times.

If given multiple times, the configs are merged and later
occurrences take precedence over previous occurrences.

[default: {}]",
    loaders::lychee_toml::LYCHEE_CONFIG_FILE
);

use clap::Arg;

/// Extension trait for `clap::Arg` to add a custom method for boolean flags that can be specified without a value (e.g. `--flag` is equivalent to `--flag=true`).
trait ArgBoolOptionalExt {
    fn optional_bool_flag(self) -> Self;
}

impl ArgBoolOptionalExt for Arg {
    fn optional_bool_flag(self) -> Self {
        self.default_missing_value("true") // Ensure `--flag` is treated as `--flag=true`
            .num_args(0..=1) // Allow the flag to be specified with no value
            .require_equals(true) // Ensure that `--flag value` is not misinterpreted as `--flag=true value`
            .value_name("false|true") // Set the value name for help messages
            .hide_possible_values(true) // We already provide the values
    }
}

/// lychee is a fast, asynchronous link checker which detects broken URLs and mail addresses
/// in local files and websites. It supports Markdown and HTML and works with other file formats.
///
/// lychee is powered by lychee-lib, the Rust library for link checking.
#[derive(Parser, Debug)]
#[command(version, about, next_display_order = None)]
pub(crate) struct LycheeOptions {
    /// Inputs for link checking (where to get links to check from).
    /// These can be: files (e.g. `README.md`), file globs (e.g. `'~/git/*/README.md'`),
    /// remote URLs (e.g. `https://example.com/README.md`), or standard input (`-`).
    /// Alternatively, use `--files-from` to read inputs from a file.
    ///
    /// NOTE: Use `--` to separate inputs from options that allow multiple arguments.
    #[arg(
        name = "inputs",
        required_unless_present = "files_from",
        required_unless_present = "generate",
        verbatim_doc_comment
    )]
    raw_inputs: Vec<String>,

    /// Configuration file to use
    #[arg(short, long = "config", value_name = "FILE_PATH")]
    #[arg(help = HELP_MSG_CONFIG_FILE)]
    pub(crate) config_files: Vec<PathBuf>,

    #[clap(flatten)]
    pub(crate) config: Config,
}

impl LycheeOptions {
    /// Get parsed inputs from options.
    // This depends on the config, which is why a method is required (we could
    // accept a `Vec<Input>` in `LycheeOptions` and do the conversion there, but
    // we wouldn't get access to `glob_ignore_case`.
    pub(crate) fn inputs(&self) -> Result<HashSet<Input>> {
        let mut all_inputs = self.raw_inputs.clone();

        // If --files-from is specified, read inputs from the file
        if let Some(files_from_path) = &self.config.files_from {
            let files_from = FilesFrom::try_from(files_from_path.as_path())
                .context("Cannot read inputs from --files-from")?;
            all_inputs.extend(files_from.inputs);
        }

        // Convert default extension to FileType if provided
        let default_file_type = self
            .config
            .default_extension
            .as_deref()
            .and_then(FileType::from_extension);

        all_inputs
            .iter()
            .map(|raw_input| {
                Input::new(raw_input, default_file_type, self.config.glob_ignore_case())
            })
            .collect::<Result<_, _>>()
            .context("Cannot parse inputs from arguments")
    }
}

// Custom deserializer function for the header field
fn deserialize_headers<'de, D>(deserializer: D) -> Result<Vec<(String, String)>, D::Error>
where
    D: Deserializer<'de>,
{
    let map = HashMap::<String, String>::deserialize(deserializer)?;
    Ok(map.into_iter().collect())
}

/// The main configuration for lychee
#[allow(clippy::struct_excessive_bools)]
#[derive(Parser, Debug, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    /// Read input filenames from the given file or stdin (if path is '-').
    ///
    /// This is useful when you have a large number of inputs that would be
    /// cumbersome to specify on the command line directly.
    ///
    /// Examples:
    ///
    ///     lychee --files-from list.txt
    ///     find . -name '*.md' | lychee --files-from -
    ///     echo 'README.md' | lychee --files-from -
    ///
    /// File Format:
    /// - Each line should contain one input (file path, URL, or glob pattern).
    /// - Lines starting with '#' are treated as comments and ignored.
    /// - Empty lines are also ignored.
    #[arg(long, value_name = "PATH", verbatim_doc_comment)]
    files_from: Option<PathBuf>,

    /// Verbose program output
    #[clap(flatten)]
    verbose: Option<Verbosity>,

    /// Do not show progress bar.
    /// This is recommended for non-interactive shells (e.g. for continuous integration)
    #[arg(short, long, verbatim_doc_comment, optional_bool_flag())]
    #[serde(default)]
    no_progress: Option<bool>,

    /// Show per-host statistics at the end of the run
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    host_stats: Option<bool>,

    /// A list of file extensions. Files not matching the specified extensions are skipped.
    ///
    /// Multiple extensions can be separated by commas. Note that if you want to check filetypes,
    /// which have multiple extensions, e.g. HTML files with both .html and .htm extensions, you need to
    /// specify both extensions explicitly.
    /// An example is: `--extensions html,htm,php,asp,aspx,jsp,cgi`.
    ///
    /// This is useful when the default extensions are not enough and you don't
    /// want to provide a long list of inputs (e.g. file1.html, file2.md, etc.)
    ///
    /// [default: md,mkd,mdx,mdown,mdwn,mkdn,mkdown,markdown,html,htm,css,txt,xml]
    #[arg(long, verbatim_doc_comment)]
    extensions: Option<FileExtensions>,

    /// This is the default file extension that is applied to files without an extension.
    ///
    /// This is useful for files without extensions or with unknown extensions.
    /// The extension will be used to determine the file type for processing.
    ///
    /// Examples:
    ///   --default-extension md
    ///   --default-extension html
    #[arg(long, value_name = "EXTENSION", verbatim_doc_comment)]
    default_extension: Option<String>,

    #[arg(help = HELP_MSG_CACHE)]
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    cache: Option<bool>,

    /// Discard all cached requests older than this duration
    ///
    /// [default: 1d]
    #[arg(long, value_parser = humantime::parse_duration)]
    #[serde(default, with = "humantime_serde")]
    max_cache_age: Option<Duration>,

    /// A list of status codes that will be ignored from the cache
    ///
    /// The following exclude range syntax is supported: [start]..[[=]end]|code. Some valid
    /// examples are:
    ///
    /// - 429 (excludes the 429 status code only)
    /// - 500.. (excludes any status code >= 500)
    /// - ..100 (excludes any status code < 100)
    /// - 500..=599 (excludes any status code from 500 to 599 inclusive)
    /// - 500..600 (excludes any status code from 500 to 600 excluding 600, same as 500..=599)
    ///
    /// Use "lychee --cache-exclude-status '429, 500..502' <inputs>..." to provide a
    /// comma-separated list of excluded status codes. This example will not cache results
    /// with a status code of 429, 500 and 501.
    #[arg(long, verbatim_doc_comment)]
    cache_exclude_status: Option<StatusCodeSelector>,

    /// Don't perform any link checking.
    /// Instead, dump all the links extracted from inputs that would be checked
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    dump: Option<bool>,

    /// Don't perform any link extraction and checking.
    /// Instead, dump all input sources from which links would be collected
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    dump_inputs: Option<bool>,

    /// Web archive to use to provide suggestions for `--suggest`.
    ///
    /// [default: wayback]
    #[arg(long, value_parser = PossibleValuesParser::new(Archive::VARIANTS).map(|s| s.parse::<Archive>().unwrap()))]
    archive: Option<Archive>,

    /// Suggest link replacements for broken links, using a web archive.
    /// The web archive can be specified with `--archive`
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    suggest: Option<bool>,

    /// Maximum number of allowed redirects
    ///
    /// [default: 10]
    #[arg(short, long)]
    max_redirects: Option<usize>,

    /// Maximum number of retries per request
    ///
    /// [default: 3]
    #[arg(long)]
    max_retries: Option<u64>,

    /// Minimum accepted TLS Version
    #[arg(long, value_parser = PossibleValuesParser::new(TlsVersion::VARIANTS).map(|s| s.parse::<TlsVersion>().unwrap()))]
    pub(crate) min_tls: Option<TlsVersion>,

    /// Maximum number of concurrent network requests
    ///
    /// [default: 128]
    #[arg(long)]
    max_concurrency: Option<usize>,

    /// Default maximum concurrent requests per host (default: 10)
    ///
    /// This limits the maximum amount of requests that are sent simultaneously
    /// to the same host. This helps to prevent overwhelming servers and
    /// running into rate-limits. Use the `hosts` option to configure this
    /// on a per-host basis.
    ///
    /// Examples:
    ///   --host-concurrency 2   # Conservative for slow APIs
    ///   --host-concurrency 20  # Aggressive for fast APIs
    #[arg(long, verbatim_doc_comment)]
    pub(crate) host_concurrency: Option<usize>,

    /// Minimum interval between requests to the same host (default: 50ms)
    ///
    /// Sets a baseline delay between consecutive requests to prevent
    /// overloading servers. The adaptive algorithm may increase this based
    /// on server responses (rate limits, errors). Use the `hosts` option
    /// to configure this on a per-host basis.
    ///
    /// Examples:
    ///   --host-request-interval 50ms   # Fast for robust APIs
    ///   --host-request-interval 1s     # Conservative for rate-limited APIs
    #[arg(long, value_parser = humantime::parse_duration, verbatim_doc_comment)]
    #[serde(default, with = "humantime_serde")]
    pub(crate) host_request_interval: Option<Duration>,

    /// Number of threads to utilize.
    /// Defaults to number of cores available to the system
    #[arg(short = 'T', long)]
    pub(crate) threads: Option<usize>,

    /// User agent
    ///
    /// [default: lychee/x.y.z]
    #[arg(short, long)]
    user_agent: Option<String>,

    /// Proceed for server connections considered insecure (invalid TLS)
    #[arg(short, long, optional_bool_flag())]
    #[serde(default)]
    insecure: Option<bool>,

    /// Only test links with the given schemes (e.g. https).
    /// Omit to check links with any other scheme.
    /// At the moment, we support http, https, file, and mailto.
    #[arg(short, long, verbatim_doc_comment)]
    #[serde(default)]
    pub(crate) scheme: Vec<String>,

    /// Only check local files and block network requests.
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    offline: Option<bool>,

    /// URLs to check (supports regex). Has preference over all excludes.
    #[arg(long)]
    #[serde(default)]
    pub(crate) include: Vec<String>,

    /// Exclude URLs and mail addresses from checking.
    /// The values are treated as regular expressions.
    #[arg(long)]
    #[serde(default)]
    pub(crate) exclude: Vec<String>,

    /// Deprecated; use `--exclude-path` instead
    #[arg(long)]
    #[serde(default)]
    pub(crate) exclude_file: Vec<String>,

    /// Exclude paths from getting checked.
    /// The values are treated as regular expressions.
    #[arg(long)]
    #[serde(default)]
    pub(crate) exclude_path: Vec<String>,

    /// Exclude all private IPs from checking.
    /// Equivalent to `--exclude-private --exclude-link-local --exclude-loopback`
    #[arg(short = 'E', long, verbatim_doc_comment, optional_bool_flag())]
    #[serde(default)]
    exclude_all_private: Option<bool>,

    /// Exclude private IP address ranges from checking
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    exclude_private: Option<bool>,

    /// Exclude link-local IP address range from checking
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    exclude_link_local: Option<bool>,

    /// Exclude loopback IP address range and localhost from checking
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    exclude_loopback: Option<bool>,

    /// Also check email addresses
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    include_mail: Option<bool>,

    /// Remap URI matching pattern to different URI
    #[serde(default)]
    #[arg(long)]
    pub(crate) remap: Vec<String>,

    /// When checking locally, attempts to locate missing files by trying the given
    /// fallback extensions. Multiple extensions can be separated by commas. Extensions
    /// will be checked in order of appearance.
    ///
    /// Example: --fallback-extensions html,htm,php,asp,aspx,jsp,cgi
    ///
    /// Note: This option takes effect on `file://` URIs which do not exist and on
    ///       `file://` URIs pointing to directories which resolve to themself (by the
    ///       --index-files logic).
    #[serde(default)]
    #[arg(long, value_delimiter = ',', verbatim_doc_comment)]
    pub(crate) fallback_extensions: Vec<String>,

    /// When checking locally, resolves directory links to a separate index file.
    /// The argument is a comma-separated list of index file names to search for. Index
    /// names are relative to the link's directory and attempted in the order given.
    ///
    /// If `--index-files` is specified, then at least one index file must exist in
    /// order for a directory link to be considered valid. Additionally, the special
    /// name `.` can be used in the list to refer to the directory itself.
    ///
    /// If unspecified (the default behavior), index files are disabled and directory
    /// links are considered valid as long as the directory exists on disk.
    ///
    /// Example 1: `--index-files index.html,readme.md` looks for index.html or readme.md
    ///            and requires that at least one exists.
    ///
    /// Example 2: `--index-files index.html,.` will use index.html if it exists, but
    ///            still accept the directory link regardless.
    ///
    /// Example 3: `--index-files ''` will reject all directory links because there are
    ///            no valid index files. This will require every link to explicitly name
    ///            a file.
    ///
    /// Note: This option only takes effect on `file://` URIs which exist and point to a directory.
    #[arg(long, value_delimiter = ',', verbatim_doc_comment)]
    pub(crate) index_files: Option<Vec<String>>,

    /// Set custom header for requests.
    ///
    /// Some websites require custom headers to be passed in order to return valid responses.
    /// You can specify custom headers in the format 'Name: Value'. For example, 'Accept: text/html'.
    /// This is the same format that other tools like curl or wget use.
    /// Multiple headers can be specified by using the flag multiple times.
    /// The specified headers are used for ALL requests.
    /// Use the `hosts` option to configure headers on a per-host basis.
    #[arg(
        short = 'H',
        long,
        // Note: We use a `Vec<(String, String)>` for headers, which is
        // unfortunate. The reason is that `clap::ArgAction::Append` collects
        // multiple values, and `clap` cannot automatically convert these tuples
        // into a `HashMap<String, String>`.
        action = clap::ArgAction::Append,
        value_parser = HeaderParser,
        value_name = "HEADER:VALUE",
        verbatim_doc_comment
    )]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_headers")]
    header: Vec<(String, String)>,

    /// A List of accepted status codes for valid links
    ///
    /// The following accept range syntax is supported: [start]..[[=]end]|code.
    /// Some valid examples are:
    ///
    /// - 200 (accepts the 200 status code only)
    /// - ..204 (accepts any status code < 204)
    /// - ..=204 (accepts any status code <= 204)
    /// - 200..=204 (accepts any status code from 200 to 204 inclusive)
    /// - 200..205 (accepts any status code from 200 to 205 excluding 205, same as 200..=204)
    ///
    /// Use "lychee --accept '200..=204, 429, 500' <inputs>..." to provide a comma-
    /// separated list of accepted status codes. This example will accept 200, 201,
    /// 202, 203, 204, 429, and 500 as valid status codes.
    ///
    /// [default: 100..=103,200..=299]
    #[arg(short, long, verbatim_doc_comment)]
    accept: Option<StatusCodeSelector>,

    /// Accept timed out requests and return exit code 0
    /// when encountering timeouts but not any other errors.
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    accept_timeouts: Option<bool>,

    /// Enable the checking of fragments in links.
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    include_fragments: Option<bool>,

    /// Website timeout in seconds from connect to response finished
    ///
    /// [default: 20]
    #[arg(short, long)]
    timeout: Option<u64>,

    /// Minimum wait time in seconds between retries of failed requests
    ///
    /// [default: 1]
    #[arg(short, long)]
    retry_wait_time: Option<u64>,

    /// Request method
    ///
    /// [default: get]
    // Using `-X` as a short param similar to curl
    #[arg(short = 'X', long)]
    method: Option<String>,

    /// Deprecated; use `--base-url` instead
    #[arg(long, value_parser = parse_base_info)]
    #[serde(skip)]
    pub(crate) base: Option<BaseInfo>,

    /// Base URL to use when resolving relative URLs in local files. If specified,
    /// relative links in local files are interpreted as being relative to the given
    /// base URL.
    ///
    /// For example, given a base URL of `https://example.com/dir/page`, the link `a`
    /// would resolve to `https://example.com/dir/a` and the link `/b` would resolve
    /// to `https://example.com/b`. This behavior is not affected by the filesystem
    /// path of the file containing these links.
    ///
    /// Note that relative URLs without a leading slash become siblings of the base
    /// URL. If, instead, the base URL ended in a slash, the link would become a child
    /// of the base URL. For example, a base URL of `https://example.com/dir/page/` and
    /// a link of `a` would resolve to `https://example.com/dir/page/a`.
    ///
    /// Basically, the base URL option resolves links as if the local files were hosted
    /// at the given base URL address.
    ///
    /// The provided base URL value must either be a URL (with scheme) or an absolute path.
    /// Note that certain URL schemes cannot be used as a base, e.g., `data` and `mailto`.
    #[arg(
        short,
        long,
        value_parser = parse_base_info,
        verbatim_doc_comment
    )]
    pub(crate) base_url: Option<BaseInfo>,

    /// Root directory to use when checking absolute links in local files. This option is
    /// required if absolute links appear in local files, otherwise those links will be
    /// flagged as errors. This must be an absolute path (i.e., one beginning with `/`).
    ///
    /// If specified, absolute links in local files are resolved by prefixing the given
    /// root directory to the requested absolute link. For example, with a root-dir of
    /// `/root/dir`, a link to `/page.html` would be resolved to `/root/dir/page.html`.
    ///
    /// This option can be specified alongside `--base-url`. If both are given, an
    /// absolute link is resolved by constructing a URL from three parts: the domain
    /// name specified in `--base-url`, followed by the `--root-dir` directory path,
    /// followed by the absolute link's own path.
    #[arg(long, verbatim_doc_comment)]
    pub(crate) root_dir: Option<PathBuf>,

    /// Basic authentication support. E.g. `http://example.com username:password`
    #[arg(long)]
    pub(crate) basic_auth: Option<Vec<BasicAuthSelector>>,

    /// GitHub API token to use when checking github.com links, to avoid rate limiting
    #[arg(long, env = "GITHUB_TOKEN", hide_env_values = true)]
    pub(crate) github_token: Option<SecretString>,

    /// Skip missing input files (default is to error if they don't exist)
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    skip_missing: Option<bool>,

    /// Do not skip files that would otherwise be ignored by
    /// '.gitignore', '.ignore', or the global ignore file.
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    no_ignore: Option<bool>,

    /// Do not skip hidden directories and files.
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    hidden: Option<bool>,

    /// Find links in verbatim sections like `pre`- and `code` blocks
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    include_verbatim: Option<bool>,

    /// Ignore case when expanding filesystem path glob inputs
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    glob_ignore_case: Option<bool>,

    /// Output file of status report
    #[arg(short, long, value_parser)]
    pub(crate) output: Option<PathBuf>,

    /// Set the output display mode. Determines how results are presented in the terminal
    ///
    /// [default: color]
    #[arg(long, value_parser = PossibleValuesParser::new(OutputMode::VARIANTS).map(|s| s.parse::<OutputMode>().unwrap()))]
    mode: Option<OutputMode>,

    /// Output format of final status report
    ///
    /// [default: compact]
    #[arg(short, long, value_parser = PossibleValuesParser::new(StatsFormat::VARIANTS).map(|s| s.parse::<StatsFormat>().unwrap()))]
    format: Option<StatsFormat>,

    /// Generate special output (e.g. the man page) instead of performing link checking
    #[arg(long, value_parser = PossibleValuesParser::new(GenerateMode::VARIANTS).map(|s| s.parse::<GenerateMode>().unwrap()))]
    pub(crate) generate: Option<GenerateMode>,

    /// When HTTPS is available, treat HTTP links as errors
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    require_https: Option<bool>,

    /// Read and write cookies using the given file. Cookies will be stored in the
    /// cookie jar and sent with requests. New cookies will be stored in the cookie jar
    /// and existing cookies will be updated.
    #[arg(long, verbatim_doc_comment)]
    pub(crate) cookie_jar: Option<PathBuf>,

    #[allow(clippy::doc_markdown)]
    /// Check WikiLinks in Markdown files, this requires specifying --base-url
    #[clap(requires = "base_url")]
    #[arg(long, optional_bool_flag())]
    #[serde(default)]
    include_wikilinks: Option<bool>,

    /// Preprocess input files with the given command.
    ///
    /// For each file input, this flag causes lychee to execute `COMMAND PATH` and process
    /// its standard output instead of the original contents of PATH. This allows you to
    /// convert files that would otherwise not be understood by lychee. The preprocessor
    /// COMMAND is only run on input files, not on standard input or URLs.
    ///
    /// To invoke programs with custom arguments or to use multiple preprocessors, use a
    /// wrapper program such as a shell script. An example script looks like this:
    ///
    /// ```
    /// #!/usr/bin/env bash
    /// case "$1" in
    /// *.pdf)
    ///     exec pdftohtml -i -s -stdout "$1"
    ///     ;;
    /// *.odt|*.docx|*.epub|*.ipynb)
    ///     exec pandoc "$1" --to=html --wrap=none
    ///     ;;
    /// *)
    ///     # identity function, output input without changes
    ///     exec cat
    ///     ;;
    /// esac
    /// ```
    #[arg(short, long, value_name = "COMMAND", verbatim_doc_comment)]
    pub(crate) preprocess: Option<Preprocessor>,

    /// Host-specific configurations from config file
    #[arg(skip)]
    #[serde(default)]
    pub(crate) hosts: HostConfigs,
}

impl Config {
    /// Try to load configuration from a file and merge into `self`.
    /// `self` has precedence over `config_file`.
    pub(crate) fn merge_file(self, config_file: &Path) -> Result<Config> {
        let config = Config::load_from_file(config_file).map_err(|e| {
            anyhow!(
                "Cannot load configuration file `{}`: {e:?}",
                config_file.display()
            )
        })?;

        Ok(self.merge(config))
    }

    fn load_from_file(path: &Path) -> Result<Config> {
        loaders::load_from_file(path)
    }

    /// Request timeout
    pub(crate) fn timeout(&self) -> Duration {
        let seconds = self.timeout.unwrap_or(DEFAULT_TIMEOUT_SECS);
        Duration::from_secs(seconds)
    }

    /// Minimum wait time between retries of failed requests
    pub(crate) fn retry_wait_time(&self) -> Duration {
        let seconds = self.retry_wait_time.unwrap_or(DEFAULT_RETRY_WAIT_TIME_SECS);
        Duration::from_secs(seconds)
    }

    /// HTTP method used for requests
    pub(crate) fn method(&self) -> String {
        let default_method: String = "get".into();
        self.method.clone().unwrap_or(default_method)
    }

    /// Maximum age of cache entries
    pub(crate) fn max_cache_age(&self) -> std::time::Duration {
        const DEFAULT_MAX_CACHE_AGE: Duration = Duration::from_secs(60 * 60 * 24); // one day
        self.max_cache_age.unwrap_or(DEFAULT_MAX_CACHE_AGE)
    }

    /// Level of verbosity for the output
    pub(crate) fn verbose(&self) -> Verbosity {
        self.verbose.clone().unwrap_or_default()
    }

    /// Selected file extensions to check
    pub(crate) fn extensions(&self) -> FileExtensions {
        self.extensions.clone().unwrap_or_default()
    }

    /// Archive to use for fallback
    pub(crate) fn archive(&self) -> Archive {
        self.archive.clone().unwrap_or_default()
    }

    /// Mode to use for formatting the output
    pub(crate) fn mode(&self) -> OutputMode {
        self.mode.clone().unwrap_or_default()
    }

    /// Format to use for the output report
    pub(crate) fn format(&self) -> StatsFormat {
        self.format.clone().unwrap_or_default()
    }

    pub(crate) const fn set_max_concurrency(&mut self, concurrency: usize) {
        self.max_concurrency = Some(concurrency);
    }

    /// Maximum number of concurrent network requests
    pub(crate) fn max_concurrency(&self) -> usize {
        const DEFAULT_MAX_CONCURRENCY: usize = 128;
        self.max_concurrency.unwrap_or(DEFAULT_MAX_CONCURRENCY)
    }

    /// Maximum number of allowed redirects
    pub(crate) fn max_redirects(&self) -> usize {
        self.max_redirects.unwrap_or(DEFAULT_MAX_REDIRECTS)
    }

    /// Maximum number of retries per request
    pub(crate) fn max_retries(&self) -> u64 {
        self.max_retries.unwrap_or(DEFAULT_MAX_RETRIES)
    }

    /// User agent used for requests
    pub(crate) fn user_agent(&self) -> String {
        self.user_agent
            .clone()
            .unwrap_or(DEFAULT_USER_AGENT.to_string())
    }

    /// Status codes that shouldn't be cached
    pub(crate) fn cache_exclude_status(&self) -> StatusCodeSelector {
        self.cache_exclude_status
            .clone()
            .unwrap_or(StatusCodeSelector::empty())
    }

    /// Whether to use the on-disk request cache
    pub(crate) fn cache(&self) -> bool {
        self.cache.unwrap_or(false)
    }

    pub(crate) fn dump(&self) -> bool {
        self.dump.unwrap_or(false)
    }

    pub(crate) fn dump_inputs(&self) -> bool {
        self.dump_inputs.unwrap_or(false)
    }

    pub(crate) fn exclude_all_private(&self) -> bool {
        self.exclude_all_private.unwrap_or(false)
    }

    pub(crate) fn exclude_link_local(&self) -> bool {
        self.exclude_link_local.unwrap_or(false)
    }

    pub(crate) fn exclude_loopback(&self) -> bool {
        self.exclude_loopback.unwrap_or(false)
    }

    pub(crate) fn exclude_private(&self) -> bool {
        self.exclude_private.unwrap_or(false)
    }

    pub(crate) fn glob_ignore_case(&self) -> bool {
        self.glob_ignore_case.unwrap_or(false)
    }

    pub(crate) fn hidden(&self) -> bool {
        self.hidden.unwrap_or(false)
    }

    pub(crate) fn host_stats(&self) -> bool {
        self.host_stats.unwrap_or(false)
    }

    pub(crate) fn include_fragments(&self) -> bool {
        self.include_fragments.unwrap_or(false)
    }

    pub(crate) fn include_mail(&self) -> bool {
        self.include_mail.unwrap_or(false)
    }

    pub(crate) fn include_verbatim(&self) -> bool {
        self.include_verbatim.unwrap_or(false)
    }

    pub(crate) fn include_wikilinks(&self) -> bool {
        self.include_wikilinks.unwrap_or(false)
    }

    pub(crate) fn insecure(&self) -> bool {
        self.insecure.unwrap_or(false)
    }

    pub(crate) fn no_ignore(&self) -> bool {
        self.no_ignore.unwrap_or(false)
    }

    pub(crate) fn no_progress(&self) -> bool {
        self.no_progress.unwrap_or(false)
    }

    pub(crate) fn offline(&self) -> bool {
        self.offline.unwrap_or(false)
    }

    pub(crate) fn require_https(&self) -> bool {
        self.require_https.unwrap_or(false)
    }

    pub(crate) fn skip_missing(&self) -> bool {
        self.skip_missing.unwrap_or(false)
    }

    pub(crate) fn suggest(&self) -> bool {
        self.suggest.unwrap_or(false)
    }

    /// Status codes that are considered successful
    pub(crate) fn accept(&self) -> StatusCodeSelector {
        self.accept
            .clone()
            .unwrap_or(StatusCodeSelector::default_accepted())
    }

    /// Whether to accept timeouts as valid results
    pub(crate) fn accept_timeouts(&self) -> bool {
        self.accept_timeouts.unwrap_or(false)
    }

    /// Custom headers to send with requests
    pub(crate) fn headers(&self) -> HashMap<String, String> {
        self.header.iter().cloned().collect()
    }

    /// Merge `self` with another `Config` where the fields of `self` take precedence
    /// over `other`.
    pub(crate) fn merge(self, other: Config) -> Config {
        let hosts = self.hosts.merge(other.hosts);
        macro_rules! merge {
            (
                option { $( $optional:ident ),* $(,)? },
                chain { $( $chainable:ident ),* $(,)? },
            ) => {
                Config {
                    hosts,
                    // Merge chainable fields (e.g. `Vec` and `HashMap`)
                    $( $chainable: self.$chainable.into_iter().chain(other.$chainable).collect(), )*
                    // Use self if present, otherwise use other
                    $( $optional: self.$optional.or(other.$optional), )*
                }
            };
        }

        merge!(
            option {
                accept,
                accept_timeouts,
                archive,
                base,
                base_url,
                basic_auth,
                cache,
                cache_exclude_status,
                cookie_jar,
                default_extension,
                dump,
                dump_inputs,
                exclude_all_private,
                exclude_link_local,
                exclude_loopback,
                exclude_private,
                github_token,
                glob_ignore_case,
                hidden,
                host_concurrency,
                host_request_interval,
                files_from,
                generate,
                host_stats,
                include_fragments,
                include_mail,
                include_verbatim,
                include_wikilinks,
                index_files,
                insecure,
                min_tls,
                no_ignore,
                no_progress,
                offline,
                output,
                preprocess,
                require_https,
                root_dir,
                skip_missing,
                suggest,
                threads,
                extensions,
                format,
                verbose,
                max_cache_age,
                max_concurrency,
                max_redirects,
                max_retries,
                method,
                mode,
                retry_wait_time,
                timeout,
                user_agent,
            },
            chain {
                exclude,
                exclude_file,
                exclude_path,
                include,
                fallback_extensions,
                remap,
                scheme,
                header,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use clap::{CommandFactory, FromArgMatches};
    use http::HeaderMap;
    use lychee_lib::ratelimit::{HostConfig, HostKey};
    use regex::Regex;

    use super::*;

    #[test]
    fn test_no_clap_default_used() {
        if Regex::new(r"(?ms)#\[arg\([^\]]*default_value")
            .unwrap()
            .is_match(&read_this_source_file())
        {
            panic!(
                r"In lychee, we avoid clap's default values.
Write a getter function instead, keep the field private and annotate the default value in the doc comment manually.
The annotated default value is then verified with a test."
            );
        }
    }

    #[test]
    fn test_no_clap_long_help_used() {
        if Regex::new(r"(?ms)#\[arg\([^\]]*long_help")
            .unwrap()
            .is_match(&read_this_source_file())
        {
            panic!(
                r"In lychee, we avoid clap's long_help.
Instead use Rust's doc comments in combination with `verbatim_doc_comment`.
This convention also simplifies our default value testing."
            );
        }
    }

    #[test]
    fn test_default_values() {
        let contents = read_this_source_file();

        let default_value_annotation = Regex::new(r"\s*\[default: (?<value>.*)\]").unwrap();
        // Matches last line of rustdoc comment, then skips a line (expected to be `#[arg(...)]`),
        // then matches a *private* field of type Option.
        let default_field =
            Regex::new(r"(?m)^\s+///(?<comment>.*)\n.*\n\s+(?<ident>\w+):\s*Option<.*>,?$")
                .unwrap();

        let undocumented_default_fields = [
            "verbose",              // the flag takes no argument
            "cache_exclude_status", // empty default
            // the following flags do not have any default values.
            // they are not public because they are only used internally.
            "default_extension",
            "files_from",
        ];

        let mut default_values = default_field
            .captures_iter(&contents)
            .map(|c| {
                (
                    c.name("comment").unwrap().as_str(),
                    c.name("ident").unwrap().as_str(),
                )
            })
            .filter(|(_,i)| !undocumented_default_fields.contains(i))
            .map(|(comment, ident)| {
                let default_value = default_value_annotation
                    .captures(comment)
                    .unwrap_or_else(|| panic!(
                        "Default value must be specified at the end of the doc comment for argument '{ident}'"
                    ))
                    .name("value")
                    .unwrap_or_else(|| panic!("Default value missing for argument '{ident}'"))
                    .as_str();

                (ident, default_value)
            }).collect::<HashMap<_,_>>();

        let default = parse_options(vec!["lychee", "-"]);

        let mut remove = |identifier: &str| {
            default_values.remove_entry(identifier)
                .unwrap_or_else(|| panic!("Option with name '{identifier}' was expected due to `check_default_values!`. Make sure it is exists as a private field of type `Option<T>`, or update the call to `check_default_values!`."))
        };

        macro_rules! check_default_values {
            ( $( $name:ident ),* $(,)? ) => {
                $(
                    let (ident, default_value) = remove(stringify!($name));
                    let flag = ident.replace("_", "-");
                    let explicit = parse_options(vec![
                        "lychee",
                        format!("--{flag}").as_str(),
                        default_value,
                        "-",
                    ]);
                    assert_eq!(
                        default.config.$name(),
                        explicit.config.$name(),
                        "Documented default value does not match the actual default value for option '{ident}'"
                    );
                )*
            };
        }

        check_default_values!(
            accept,
            archive,
            extensions,
            format,
            max_concurrency,
            max_redirects,
            max_retries,
            mode,
            retry_wait_time,
            timeout,
        );

        // We document `lychee/x.y.z` as default instead of the actual version
        assert_eq!(remove("user_agent"), ("user_agent", "lychee/x.y.z"));

        assert_eq!(
            default_values,
            HashMap::new(),
            "Untested default values found. Add them to this test."
        );
    }

    #[test]
    #[expect(clippy::bool_assert_comparison)]
    fn test_bool_flags() {
        // Values for boolean flags can be specified explicitly.
        let explicit = parse_options(vec!["lychee", "-", "--dump=false"]);
        assert_eq!(explicit.config.dump(), false);

        let explicit = parse_options(vec!["lychee", "-", "--dump=true"]);
        assert_eq!(explicit.config.dump(), true);

        // Or implicitly
        let implicit = parse_options(vec!["lychee", "-", "--dump"]);
        assert_eq!(implicit.config.dump(), true);

        // They default to `false`
        let default = parse_options(vec!["lychee", "-"]);
        assert_eq!(default.config.dump(), false);
    }

    #[test]
    fn test_header_parsing_and_merging() {
        // Simulate commandline arguments with multiple headers
        let args = vec![
            "lychee",
            "--header",
            "Accept: text/html",
            "--header",
            "X-Test: check=this",
            "input.md",
        ];

        // Parse the arguments
        let opts = crate::LycheeOptions::parse_from(args);

        // Check that the headers were collected correctly
        assert_eq!(
            opts.config.headers(),
            HashMap::from([
                ("accept".to_string(), "text/html".to_string()),
                ("x-test".to_string(), "check=this".to_string()),
            ])
        );
    }

    #[test]
    fn test_merge_headers_with_config() {
        let toml = Config {
            header: vec![
                ("Accept".to_string(), "text/html".to_string()),
                ("X-Test".to_string(), "check=this".to_string()),
            ],
            ..Default::default()
        };

        // Set X-Test and see if it gets overwritten
        let cli = Config {
            header: vec![("X-Test".to_string(), "check=that".to_string())],
            ..Default::default()
        }
        .merge(toml);

        assert_eq!(
            cli.headers(),
            HashMap::from([
                ("Accept".to_string(), "text/html".to_string()),
                ("X-Test".to_string(), "check=this".to_string()),
            ])
        );
    }

    #[test]
    fn test_merge_hosts() {
        let host_key = HostKey::from("hi");

        let secrets = Config {
            hosts: HostConfigs::from([(
                host_key.clone(),
                HostConfig {
                    concurrency: Some(1),
                    request_interval: None,
                    headers: HeaderMap::from_header_pairs(&HashMap::from([(
                        "password".into(),
                        "very secret".into(),
                    )]))
                    .unwrap(),
                },
            )]),
            ..Default::default()
        };

        let main = Config {
            hosts: HostConfigs::from([(
                host_key.clone(),
                HostConfig {
                    concurrency: Some(42),
                    request_interval: Some(Duration::ZERO),
                    headers: HeaderMap::from_header_pairs(&HashMap::from([(
                        "hi".into(),
                        "there".into(),
                    )]))
                    .unwrap(),
                },
            )]),
            ..Default::default()
        }
        .merge(secrets);

        assert_eq!(
            main.hosts,
            HostConfigs::from([(
                host_key.clone(),
                HostConfig {
                    concurrency: Some(42), // main config takes precedence
                    request_interval: Some(Duration::ZERO),
                    headers: HeaderMap::from_header_pairs(&HashMap::from([
                        ("password".into(), "very secret".into()),
                        ("hi".into(), "there".into()),
                    ]))
                    .unwrap()
                }
            )])
        );
    }

    fn read_this_source_file() -> String {
        fs::read_to_string("./src/config/mod.rs")
            .expect("Unable to read this source code file to string")
    }

    fn parse_options(args: Vec<&str>) -> LycheeOptions {
        let mut matches = <LycheeOptions as CommandFactory>::command().get_matches_from(args);
        <LycheeOptions as FromArgMatches>::from_arg_matches_mut(&mut matches).unwrap()
    }
}
