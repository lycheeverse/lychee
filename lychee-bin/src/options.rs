use crate::files_from::FilesFrom;
use crate::generate::GenerateMode;
use crate::parse::parse_base_info;
use crate::verbosity::Verbosity;
use anyhow::{Context, Error, Result, anyhow};
use clap::builder::PossibleValuesParser;
use clap::{Parser, builder::TypedValueParser};
use const_format::{concatcp, formatcp};
use http::{
    HeaderMap,
    header::{HeaderName, HeaderValue},
};
use lychee_lib::Preprocessor;
use lychee_lib::ratelimit::HostConfigs;
use lychee_lib::{
    BaseInfo, BasicAuthSelector, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_RETRIES,
    DEFAULT_RETRY_WAIT_TIME_SECS, DEFAULT_TIMEOUT_SECS, DEFAULT_USER_AGENT, FileExtensions,
    FileType, Input, StatusCodeSelector, archive::Archive,
};
use reqwest::tls;
use secrecy::SecretString;
use serde::{Deserialize, Deserializer};
use std::collections::{HashMap, HashSet};
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

#[derive(
    Debug, Deserialize, Default, Clone, Display, EnumIter, EnumString, VariantNames, PartialEq, Eq,
)]
#[non_exhaustive]
pub(crate) enum TlsVersion {
    #[serde(rename = "TLSv1_0")]
    #[strum(serialize = "TLSv1_0")]
    V1_0,
    #[serde(rename = "TLSv1_1")]
    #[strum(serialize = "TLSv1_1")]
    V1_1,
    #[serde(rename = "TLSv1_2")]
    #[strum(serialize = "TLSv1_2")]
    #[default]
    V1_2,
    #[serde(rename = "TLSv1_3")]
    #[strum(serialize = "TLSv1_3")]
    V1_3,
}
impl From<TlsVersion> for tls::Version {
    fn from(ver: TlsVersion) -> Self {
        match ver {
            TlsVersion::V1_0 => tls::Version::TLS_1_0,
            TlsVersion::V1_1 => tls::Version::TLS_1_1,
            TlsVersion::V1_2 => tls::Version::TLS_1_2,
            TlsVersion::V1_3 => tls::Version::TLS_1_3,
        }
    }
}

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
            _ => Err(anyhow!("Unknown format {format}")),
        }
    }
}

/// The different formatter modes
///
/// This decides over whether to use color,
/// emojis, or plain text for the output.
#[derive(
    Debug, Deserialize, Default, Clone, Display, EnumIter, EnumString, VariantNames, PartialEq,
)]
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

    /// Task output.
    ///
    /// This mode uses Markdown-styled checkboxes to represent the status of the requests.
    /// Some people may find this mode more intuitive and useful for task tracking.
    #[serde(rename = "task")]
    #[strum(serialize = "task", ascii_case_insensitive)]
    Task,
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
}

// Macro for merging configuration values
macro_rules! fold_in {
    ($cli:ident , $toml:ident ; $ty:ident { $(..$ignore:ident,)* $( $key:ident : $default:expr, )* } ) => {
        if (false) {
            #[allow(dead_code, unused, clippy::diverging_sub_expression)]
            let _check_fold_in_exhaustivity = $ty {
                $($key: unreachable!(), )*
                $($ignore: unreachable!(), )*
            };
        };
        $(
            if $cli.$key == $default && $toml.$key != $default {
                $cli.$key = $toml.$key;
            }
        )*
    };
}

/// Parse a single header into a [`HeaderName`] and [`HeaderValue`]
///
/// Headers are expected to be in format `Header-Name: Header-Value`.
/// The header name and value are trimmed of whitespace.
///
/// If the header contains multiple colons, the part after the first colon is
/// considered the value.
///
/// # Errors
///
/// This fails if the header does not contain exactly one `:` character or
/// if the header name contains non-ASCII characters.
fn parse_single_header(header: &str) -> Result<(HeaderName, HeaderValue)> {
    let parts: Vec<&str> = header.splitn(2, ':').collect();
    match parts.as_slice() {
        [name, value] => {
            let name = name.trim();
            let name = HeaderName::from_str(name)
                .map_err(|e| anyhow!("Unable to convert header name '{name}': {e}"))?;
            let value = HeaderValue::from_str(value.trim())
                .map_err(|e| anyhow!("Unable to read value of header with name '{name}': {e}"))?;
            Ok((name, value))
        }
        _ => Err(anyhow!(
            "Invalid header format. Expected colon-separated string in the format 'HeaderName: HeaderValue'"
        )),
    }
}

/// Parses a single HTTP header into a tuple of (String, String)
///
/// This does NOT merge multiple headers into one.
#[derive(Clone, Debug)]
struct HeaderParser;

impl TypedValueParser for HeaderParser {
    type Value = (String, String);

    fn parse_ref(
        &self,
        _cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let header_str = value.to_str().ok_or_else(|| {
            clap::Error::raw(
                clap::error::ErrorKind::InvalidValue,
                "Header value contains invalid UTF-8",
            )
        })?;

        match parse_single_header(header_str) {
            Ok((name, value)) => {
                let Ok(value) = value.to_str() else {
                    return Err(clap::Error::raw(
                        clap::error::ErrorKind::InvalidValue,
                        "Header value contains invalid UTF-8",
                    ));
                };

                Ok((name.to_string(), value.to_string()))
            }
            Err(e) => Err(clap::Error::raw(
                clap::error::ErrorKind::InvalidValue,
                e.to_string(),
            )),
        }
    }
}

impl clap::builder::ValueParserFactory for HeaderParser {
    type Parser = HeaderParser;
    fn value_parser() -> Self::Parser {
        HeaderParser
    }
}

/// Extension trait for converting a Vec of header pairs to a `HeaderMap`
pub(crate) trait HeaderMapExt {
    /// Convert a collection of header key-value pairs to a `HeaderMap`
    fn from_header_pairs(headers: &[(String, String)]) -> Result<HeaderMap, Error>;
}

impl HeaderMapExt for HeaderMap {
    fn from_header_pairs(headers: &[(String, String)]) -> Result<HeaderMap, Error> {
        let mut header_map = HeaderMap::new();
        for (name, value) in headers {
            let header_name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|e| anyhow!("Invalid header name '{name}': {e}"))?;
            let header_value = HeaderValue::from_str(value)
                .map_err(|e| anyhow!("Invalid header value '{value}': {e}"))?;
            header_map.insert(header_name, header_value);
        }
        Ok(header_map)
    }
}

/// lychee is a fast, asynchronous link checker which detects broken URLs and mail addresses
/// in local files and websites. It supports Markdown and HTML and works well
/// with many plain text file formats.
///
/// lychee is powered by lychee-lib, the Rust library for link checking.
#[derive(Parser, Debug)]
#[command(version, about, next_display_order = None)]
pub(crate) struct LycheeOptions {
    /// Inputs for link checking (where to get links to check from).
    #[arg(
        name = "inputs",
        required_unless_present = "files_from",
        required_unless_present = "generate",
        long_help = "Inputs for link checking (where to get links to check from). These can be:
files (e.g. `README.md`), file globs (e.g. `'~/git/*/README.md'`), remote URLs
(e.g. `https://example.com/README.md`), or standard input (`-`). Alternatively,
use `--files-from` to read inputs from a file.

NOTE: Use `--` to separate inputs from options that allow multiple arguments."
    )]
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
            .map(|raw_input| Input::new(raw_input, default_file_type, self.config.glob_ignore_case))
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
    #[arg(
        long,
        value_name = "PATH",
        long_help = "Read input filenames from the given file or stdin (if path is '-').

This is useful when you have a large number of inputs that would be
cumbersome to specify on the command line directly.

Examples:

    lychee --files-from list.txt
    find . -name '*.md' | lychee --files-from -
    echo 'README.md' | lychee --files-from -

File Format:
- Each line should contain one input (file path, URL, or glob pattern).
- Lines starting with '#' are treated as comments and ignored.
- Empty lines are also ignored."
    )]
    files_from: Option<PathBuf>,

    /// Verbose program output
    #[clap(flatten)]
    #[serde(default = "verbosity")]
    pub(crate) verbose: Verbosity,

    /// Do not show progress bar.
    /// This is recommended for non-interactive shells (e.g. for continuous integration)
    #[arg(short, long, verbatim_doc_comment)]
    #[serde(default)]
    pub(crate) no_progress: bool,

    /// Show per-host statistics at the end of the run
    #[arg(long)]
    #[serde(default)]
    pub(crate) host_stats: bool,

    /// A list of file extensions. Files not matching the specified extensions are skipped.
    ///
    /// E.g. a user can specify `--extensions html,htm,php,asp,aspx,jsp,cgi`
    /// to check for links in files with these extensions.
    ///
    /// This is useful when the default extensions are not enough and you don't
    /// want to provide a long list of inputs (e.g. file1.html, file2.md, etc.)
    #[arg(
        long,
        default_value_t = FileExtensions::default(),
        long_help = "Test the specified file extensions for URIs when checking files locally.

Multiple extensions can be separated by commas. Note that if you want to check filetypes,
which have multiple extensions, e.g. HTML files with both .html and .htm extensions, you need to
specify both extensions explicitly."
    )]
    #[serde(default = "FileExtensions::default")]
    pub(crate) extensions: FileExtensions,

    /// This is the default file extension that is applied to files without an extension.
    ///
    /// This is useful for files without extensions or with unknown extensions.
    /// The extension will be used to determine the file type for processing.
    ///
    /// Examples:
    ///   --default-extension md
    ///   --default-extension html
    #[arg(long, value_name = "EXTENSION", verbatim_doc_comment)]
    #[serde(default)]
    pub(crate) default_extension: Option<String>,

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
        long_help = "A list of status codes that will be ignored from the cache

The following exclude range syntax is supported: [start]..[[=]end]|code. Some valid
examples are:

- 429 (excludes the 429 status code only)
- 500.. (excludes any status code >= 500)
- ..100 (excludes any status code < 100)
- 500..=599 (excludes any status code from 500 to 599 inclusive)
- 500..600 (excludes any status code from 500 to 600 excluding 600, same as 500..=599)

Use \"lychee --cache-exclude-status '429, 500..502' <inputs>...\" to provide a
comma-separated list of excluded status codes. This example will not cache results
with a status code of 429, 500 and 501."
    )]
    pub(crate) cache_exclude_status: Option<StatusCodeSelector>,

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

    /// Minimum accepted TLS Version
    #[arg(long, value_parser = PossibleValuesParser::new(TlsVersion::VARIANTS).map(|s| s.parse::<TlsVersion>().unwrap()))]
    #[serde(default)]
    pub(crate) min_tls: Option<TlsVersion>,

    /// Maximum number of concurrent network requests
    #[arg(long, default_value = &MAX_CONCURRENCY_STR)]
    #[serde(default = "max_concurrency")]
    pub(crate) max_concurrency: usize,

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
    #[serde(default)]
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
    #[arg(
        short,
        long,
        long_help = "Only test links with the given schemes (e.g. https). Omit to check links with
any other scheme. At the moment, we support http, https, file, and mailto."
    )]
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

    /// Also check email addresses
    #[arg(long)]
    #[serde(default)]
    pub(crate) include_mail: bool,

    /// Remap URI matching pattern to different URI
    #[serde(default)]
    #[arg(long)]
    pub(crate) remap: Vec<String>,

    /// Automatically append file extensions to `file://` URIs for non-existing paths
    #[serde(default)]
    #[arg(
        long,
        value_delimiter = ',',
        long_help = "When checking locally, attempts to locate missing files by trying the given
fallback extensions. Multiple extensions can be separated by commas. Extensions
will be checked in order of appearance.

Example: --fallback-extensions html,htm,php,asp,aspx,jsp,cgi

Note: This option takes effect on `file://` URIs which do not exist and on
      `file://` URIs pointing to directories which resolve to themself (by the
      --index-files logic)."
    )]
    pub(crate) fallback_extensions: Vec<String>,

    /// Resolve local directory links to specified index files within the directory
    #[serde(default)]
    #[arg(
        long,
        value_delimiter = ',',
        long_help = "When checking locally, resolves directory links to a separate index file.
The argument is a comma-separated list of index file names to search for. Index
names are relative to the link's directory and attempted in the order given.

If `--index-files` is specified, then at least one index file must exist in
order for a directory link to be considered valid. Additionally, the special
name `.` can be used in the list to refer to the directory itself.

If unspecified (the default behavior), index files are disabled and directory
links are considered valid as long as the directory exists on disk.

Example 1: `--index-files index.html,readme.md` looks for index.html or readme.md
           and requires that at least one exists.

Example 2: `--index-files index.html,.` will use index.html if it exists, but
           still accept the directory link regardless.

Example 3: `--index-files ''` will reject all directory links because there are
           no valid index files. This will require every link to explicitly name
           a file.

Note: This option only takes effect on `file://` URIs which exist and point to a directory."
    )]
    pub(crate) index_files: Option<Vec<String>>,

    /// Set custom header for requests
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
        long_help = "Set custom header for requests

Some websites require custom headers to be passed in order to return valid responses.
You can specify custom headers in the format 'Name: Value'. For example, 'Accept: text/html'.
This is the same format that other tools like curl or wget use.
Multiple headers can be specified by using the flag multiple times.
The specified headers are used for ALL requests.
Use the `hosts` option to configure headers on a per-host basis."
    )]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_headers")]
    pub header: Vec<(String, String)>,

    /// A List of accepted status codes for valid links
    #[arg(
        short,
        long,
        long_help = "A List of accepted status codes for valid links

The following accept range syntax is supported: [start]..[[=]end]|code. Some valid
examples are:

- 200 (accepts the 200 status code only)
- ..204 (accepts any status code < 204)
- ..=204 (accepts any status code <= 204)
- 200..=204 (accepts any status code from 200 to 204 inclusive)
- 200..205 (accepts any status code from 200 to 205 excluding 205, same as 200..=204)

Use \"lychee --accept '200..=204, 429, 500' <inputs>...\" to provide a comma-
separated list of accepted status codes. This example will accept 200, 201,
202, 203, 204, 429, and 500 as valid status codes.
Defaults to '100..=103,200..=299' if the user provides no value."
    )]
    pub(crate) accept: Option<StatusCodeSelector>,

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

    /// Deprecated; use `--base-url` instead
    #[arg(long, value_parser = parse_base_info)]
    #[serde(skip)]
    pub(crate) base: Option<BaseInfo>,

    /// Base URL used to resolve relative URLs in local files.
    /// Example: <https://example.com>
    #[arg(
        short,
        long,
        value_parser = parse_base_info,
        long_help = "Base URL to use when resolving relative URLs in local files. If specified,
relative links in local files are interpreted as being relative to the given
base URL.

For example, given a base URL of `https://example.com/dir/page`, the link `a`
would resolve to `https://example.com/dir/a` and the link `/b` would resolve
to `https://example.com/b`. This behavior is not affected by the filesystem
path of the file containing these links.

Note that relative URLs without a leading slash become siblings of the base
URL. If, instead, the base URL ended in a slash, the link would become a child
of the base URL. For example, a base URL of `https://example.com/dir/page/` and
a link of `a` would resolve to `https://example.com/dir/page/a`.

Basically, the base URL option resolves links as if the local files were hosted
at the given base URL address.

The provided base URL value must either be a URL (with scheme) or an absolute path.
Note that certain URL schemes cannot be used as a base, e.g., `data` and `mailto`."
    )]
    #[serde(default)]
    pub(crate) base_url: Option<BaseInfo>,

    /// Root directory to use when checking absolute links in local files.
    /// Must be an absolute path.
    #[arg(
        long,
        long_help = "Root directory to use when checking absolute links in local files. This option is
required if absolute links appear in local files, otherwise those links will be
flagged as errors. This must be an absolute path (i.e., one beginning with `/`).

If specified, absolute links in local files are resolved by prefixing the given
root directory to the requested absolute link. For example, with a root-dir of
`/root/dir`, a link to `/page.html` would be resolved to `/root/dir/page.html`.

This option can be specified alongside `--base-url`. If both are given, an
absolute link is resolved by constructing a URL from three parts: the domain
name specified in `--base-url`, followed by the `--root-dir` directory path,
followed by the absolute link's own path."
    )]
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

    /// Generate special output (e.g. the man page) instead of performing link checking
    #[arg(long, value_parser = PossibleValuesParser::new(GenerateMode::VARIANTS).map(|s| s.parse::<GenerateMode>().unwrap()))]
    pub(crate) generate: Option<GenerateMode>,

    /// When HTTPS is available, treat HTTP links as errors
    #[arg(long)]
    #[serde(default)]
    pub(crate) require_https: bool,

    /// Read and write cookies using the given file
    #[arg(
        long,
        long_help = "Tell lychee to read cookies from the given file. Cookies will be stored in the
cookie jar and sent with requests. New cookies will be stored in the cookie jar
and existing cookies will be updated."
    )]
    #[serde(default)]
    pub(crate) cookie_jar: Option<PathBuf>,

    #[allow(clippy::doc_markdown)]
    /// Check WikiLinks in Markdown files, this requires specifying --base-url
    #[clap(requires = "base_url")]
    #[arg(long)]
    #[serde(default)]
    pub(crate) include_wikilinks: bool,

    /// Preprocess input files.
    #[arg(
        short,
        long,
        value_name = "COMMAND",
        long_help = r#"Preprocess input files.
For each file input, this flag causes lychee to execute `COMMAND PATH` and process
its standard output instead of the original contents of PATH. This allows you to
convert files that would otherwise not be understood by lychee. The preprocessor
COMMAND is only run on input files, not on standard input or URLs.

To invoke programs with custom arguments or to use multiple preprocessors, use a
wrapper program such as a shell script. An example script looks like this:

#!/usr/bin/env bash
case "$1" in
*.pdf)
    exec pdftohtml -i -s -stdout "$1"
    ;;
*.odt|*.docx|*.epub|*.ipynb)
    exec pandoc "$1" --to=html --wrap=none
    ;;
*)
    # identity function, output input without changes
    exec cat
    ;;
esac"#
    )]
    #[serde(default)]
    pub(crate) preprocess: Option<Preprocessor>,

    /// Host-specific configurations from config file
    #[arg(skip)]
    #[serde(default)]
    pub(crate) hosts: HostConfigs,
}

impl Config {
    /// Special handling for merging headers
    ///
    /// Overwrites existing headers in `self` with the values from `other`.
    fn merge_headers(&mut self, other: &[(String, String)]) {
        let self_map = self.header.iter().cloned().collect::<HashMap<_, _>>();
        let other_map = other.iter().cloned().collect::<HashMap<_, _>>();

        // Merge the two maps, with `other` taking precedence
        let merged_map: HashMap<_, _> = self_map.into_iter().chain(other_map).collect();

        // Convert the merged map back to a Vec of tuples
        self.header = merged_map.into_iter().collect();
    }

    /// Load configuration from a file
    pub(crate) fn load_from_file(path: &Path) -> Result<Config> {
        // Read configuration file
        let contents = fs::read_to_string(path)?;
        toml::from_str(&contents).with_context(|| "Failed to parse configuration file")
    }

    /// Merge the configuration from TOML into the CLI configuration
    pub(crate) fn merge(&mut self, toml: Config) {
        // Special handling for headers before fold_in!
        self.merge_headers(&toml.header);

        // If the config file has a value for the GitHub token, but the CLI
        // doesn't, use the token from the config file.
        // This is outside of fold_in! because SecretBox doesn't implement Eq.
        if self.github_token.is_none() && toml.github_token.is_some() {
            self.github_token = toml.github_token;
        }

        // Hosts configuration is only available in TOML for now (not in the CLI)
        // That's because it's a bit complex to specify on the command line and
        // we didn't come up with a good syntax for it yet.
        self.hosts = toml.hosts;

        // NOTE: if you see an error within this macro call, check to make sure that
        // that the fields provided to fold_in! match all the fields of the Config struct.
        fold_in! {
            // Destination and source configs
            self, toml;

            Config {
                // Keys which are handled outside of fold_in
                ..header,
                ..github_token,
                ..hosts,

                // Keys with defaults to assign
                accept: None,
                archive: None,
                base: None,
                base_url: None,
                basic_auth: None,
                cache: false,
                cache_exclude_status: None,
                cookie_jar: None,
                default_extension: None,
                host_concurrency: None,
                host_request_interval: None,
                dump: false,
                dump_inputs: false,
                exclude: Vec::<String>::new(),
                exclude_all_private: false,
                exclude_file: Vec::<String>::new(), // deprecated
                exclude_link_local: false,
                exclude_loopback: false,
                exclude_path: Vec::<String>::new(),
                exclude_private: false,
                extensions: FileType::default_extensions(),
                fallback_extensions: Vec::<String>::new(),
                files_from: None,
                format: StatsFormat::default(),
                generate: None,
                glob_ignore_case: false,
                hidden: false,
                host_stats: false,
                include: Vec::<String>::new(),
                include_fragments: false,
                include_mail: false,
                include_verbatim: false,
                include_wikilinks: false,
                index_files: None,
                insecure: false,
                max_cache_age: humantime::parse_duration(DEFAULT_MAX_CACHE_AGE).unwrap(),
                max_concurrency: DEFAULT_MAX_CONCURRENCY,
                max_redirects: DEFAULT_MAX_REDIRECTS,
                max_retries: DEFAULT_MAX_RETRIES,
                method: DEFAULT_METHOD,
                min_tls: None,
                mode: OutputMode::Color,
                no_ignore: false,
                no_progress: false,
                offline: false,
                output: None,
                preprocess: None,
                remap: Vec::<String>::new(),
                require_https: false,
                retry_wait_time: DEFAULT_RETRY_WAIT_TIME_SECS,
                root_dir: None,
                scheme: Vec::<String>::new(),
                skip_missing: false,
                suggest: false,
                threads: None,
                timeout: DEFAULT_TIMEOUT_SECS,
                user_agent: DEFAULT_USER_AGENT,
                verbose: Verbosity::default(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_parse_custom_headers() {
        assert_eq!(
            parse_single_header("accept:text/html").unwrap(),
            (
                HeaderName::from_static("accept"),
                HeaderValue::from_static("text/html")
            )
        );
    }

    #[test]
    fn test_parse_custom_header_multiple_colons() {
        assert_eq!(
            parse_single_header("key:x-test:check=this").unwrap(),
            (
                HeaderName::from_static("key"),
                HeaderValue::from_static("x-test:check=this")
            )
        );
    }

    #[test]
    fn test_parse_custom_headers_with_equals() {
        assert_eq!(
            parse_single_header("key:x-test=check=this").unwrap(),
            (
                HeaderName::from_static("key"),
                HeaderValue::from_static("x-test=check=this")
            )
        );
    }

    #[test]
    /// We should not reveal potentially sensitive data contained in the headers.
    /// See: [#1297](https://github.com/lycheeverse/lychee/issues/1297)
    fn test_does_not_echo_sensitive_data() {
        let error = parse_single_header("My-Header💣: secret")
            .expect_err("Should not allow unicode as key");
        assert!(!error.to_string().contains("secret"));

        let error = parse_single_header("secret").expect_err("Should fail when no `:` given");
        assert!(!error.to_string().contains("secret"));
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
        let headers = &opts.config.header;
        assert_eq!(headers.len(), 2);

        // Convert to HashMap for easier testing
        let header_map: HashMap<String, String> = headers.iter().cloned().collect();
        assert_eq!(header_map["accept"], "text/html");
        assert_eq!(header_map["x-test"], "check=this");
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
        let mut cli = Config {
            header: vec![("X-Test".to_string(), "check=that".to_string())],
            ..Default::default()
        };
        cli.merge(toml);

        assert_eq!(cli.header.len(), 2);

        // Sort vector before assert
        cli.header.sort();

        assert_eq!(
            cli.header,
            vec![
                ("Accept".to_string(), "text/html".to_string()),
                ("X-Test".to_string(), "check=this".to_string()),
            ]
        );
    }
}
