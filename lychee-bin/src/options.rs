use crate::files_from::FilesFrom;
use crate::generate::GenerateMode;
use crate::parse::parse_base_info;
use crate::verbosity::Verbosity;
use anyhow::{Context, Error, Result, anyhow};
use clap::builder::PossibleValuesParser;
use clap::{Parser, builder::TypedValueParser};
use const_format::formatcp;
use http::{
    HeaderMap,
    header::{HeaderName, HeaderValue},
};
use lychee_lib::ratelimit::HostConfigs;
use lychee_lib::{
    BaseInfo, BasicAuthSelector, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_RETRIES,
    DEFAULT_RETRY_WAIT_TIME_SECS, DEFAULT_TIMEOUT_SECS, FileExtensions, FileType, Input,
    StatusCodeSelector, archive::Archive,
};
use lychee_lib::{DEFAULT_USER_AGENT, Preprocessor};
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

const HELP_MSG_CACHE: &str = formatcp!(
    "Use request cache stored on disk at `{}`",
    LYCHEE_CACHE_FILE,
);
// We use a custom help message here because we want to show the default
// value of the config file, but also be able to check if the user has
// provided a custom value. If they didn't, we won't throw an error if
// the file doesn't exist.
const HELP_MSG_CONFIG_FILE: &str = formatcp!(
    "Configuration file to use. Can be specified multiple times.

If given multiple times, the configs are merged and later
occurrences take precedence over previous occurrences.

[default: {}]",
    LYCHEE_CONFIG_FILE,
);
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
    fn from_header_pairs(headers: &HashMap<String, String>) -> Result<HeaderMap, Error>;
}

impl HeaderMapExt for HeaderMap {
    fn from_header_pairs(headers: &HashMap<String, String>) -> Result<HeaderMap, Error> {
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
    #[arg(short, long, verbatim_doc_comment)]
    #[serde(default)]
    pub(crate) no_progress: bool,

    /// Show per-host statistics at the end of the run
    #[arg(long)]
    #[serde(default)]
    pub(crate) host_stats: bool,

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
    /// [default: md,mkd,mdx,mdown,mdwn,mkdn,mkdown,markdown,html,htm,css,txt]
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
    #[arg(long)]
    #[serde(default)]
    pub(crate) cache: bool,

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
    #[arg(long)]
    #[serde(default)]
    pub(crate) dump: bool,

    /// Don't perform any link extraction and checking.
    /// Instead, dump all input sources from which links would be collected
    #[arg(long)]
    #[serde(default)]
    pub(crate) dump_inputs: bool,

    /// Web archive to use to provide suggestions for `--suggest`.
    ///
    /// [default: wayback]
    #[arg(long, value_parser = PossibleValuesParser::new(Archive::VARIANTS).map(|s| s.parse::<Archive>().unwrap()))]
    archive: Option<Archive>,

    /// Suggest link replacements for broken links, using a web archive.
    /// The web archive can be specified with `--archive`
    #[arg(long)]
    #[serde(default)]
    pub(crate) suggest: bool,

    /// Maximum number of allowed redirects
    ///
    /// [default: 5]
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
    #[arg(short, long)]
    #[serde(default)]
    pub(crate) insecure: bool,

    /// Only test links with the given schemes (e.g. https).
    /// Omit to check links with any other scheme.
    /// At the moment, we support http, https, file, and mailto.
    #[arg(short, long, verbatim_doc_comment)]
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

    /// Accept timeout as a valid response
    #[arg(long)]
    #[serde(default)]
    pub(crate) accept_timeout: bool,

    /// Enable the checking of fragments in links.
    #[arg(long)]
    #[serde(default)]
    pub(crate) include_fragments: bool,

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
    #[arg(long)]
    #[serde(default)]
    pub(crate) require_https: bool,

    /// Read and write cookies using the given file. Cookies will be stored in the
    /// cookie jar and sent with requests. New cookies will be stored in the cookie jar
    /// and existing cookies will be updated.
    #[arg(long, verbatim_doc_comment)]
    pub(crate) cookie_jar: Option<PathBuf>,

    #[allow(clippy::doc_markdown)]
    /// Check WikiLinks in Markdown files, this requires specifying --base-url
    #[clap(requires = "base_url")]
    #[arg(long)]
    #[serde(default)]
    pub(crate) include_wikilinks: bool,

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
        // Read configuration file
        let contents = fs::read_to_string(path)?;
        toml::from_str(&contents).with_context(|| "Failed to parse configuration file")
    }

    pub(crate) fn timeout(&self) -> Duration {
        let seconds = self.timeout.unwrap_or(DEFAULT_TIMEOUT_SECS);
        Duration::from_secs(seconds)
    }

    pub(crate) fn retry_wait_time(&self) -> Duration {
        let seconds = self.retry_wait_time.unwrap_or(DEFAULT_RETRY_WAIT_TIME_SECS);
        Duration::from_secs(seconds)
    }

    pub(crate) fn method(&self) -> String {
        let default_method: String = "get".into();
        self.method.clone().unwrap_or(default_method)
    }

    pub(crate) fn max_cache_age(&self) -> std::time::Duration {
        const DEFAULT_MAX_CACHE_AGE: Duration = Duration::from_secs(60 * 60 * 24); // one day
        self.max_cache_age.unwrap_or(DEFAULT_MAX_CACHE_AGE)
    }

    pub(crate) fn verbose(&self) -> Verbosity {
        self.verbose.clone().unwrap_or_default()
    }

    pub(crate) fn extensions(&self) -> FileExtensions {
        self.extensions.clone().unwrap_or_default()
    }

    pub(crate) fn archive(&self) -> Archive {
        self.archive.clone().unwrap_or_default()
    }

    pub(crate) fn mode(&self) -> OutputMode {
        self.mode.clone().unwrap_or_default()
    }

    pub(crate) fn format(&self) -> StatsFormat {
        self.format.clone().unwrap_or_default()
    }

    pub(crate) fn max_concurrency(&self) -> usize {
        const DEFAULT_MAX_CONCURRENCY: usize = 128;
        self.max_concurrency.unwrap_or(DEFAULT_MAX_CONCURRENCY)
    }

    pub(crate) fn max_redirects(&self) -> usize {
        self.max_redirects.unwrap_or(DEFAULT_MAX_REDIRECTS)
    }

    pub(crate) fn max_retries(&self) -> u64 {
        self.max_retries.unwrap_or(DEFAULT_MAX_RETRIES)
    }

    pub(crate) fn user_agent(&self) -> String {
        self.user_agent
            .clone()
            .unwrap_or(DEFAULT_USER_AGENT.to_string())
    }

    pub(crate) fn cache_exclude_status(&self) -> StatusCodeSelector {
        self.cache_exclude_status
            .clone()
            .unwrap_or(StatusCodeSelector::empty())
    }

    pub(crate) fn accept(&self) -> StatusCodeSelector {
        self.accept
            .clone()
            .unwrap_or(StatusCodeSelector::default_accepted())
    }

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
                bool { $( $bool:ident ),* $(,)? },
            ) => {
                Config {
                    hosts,
                    // Merge chainable fields (e.g. `Vec` and `HashMap`)
                    $( $chainable: self.$chainable.into_iter().chain(other.$chainable).collect(), )*
                    // Use self if present, otherwise use other
                    $( $optional: self.$optional.or(other.$optional), )*
                    // Use `true` when self or other is `true`.
                    // Note that this has the drawback, that a value cannot be overwritten with
                    // `false` in the merge chain, as there is no way to distinguish
                    // between "default" `false` and user-provided `false`.
                    // We would have to use `Option<bool>` in order to do that.
                    // See: https://github.com/lycheeverse/lychee/issues/2051
                    $( $bool: self.$bool || other.$bool, )*
                }
            };
        }

        merge!(
            option {
                accept,
                archive,
                base,
                base_url,
                basic_auth,
                cache_exclude_status,
                cookie_jar,
                default_extension,
                github_token,
                host_concurrency,
                host_request_interval,
                files_from,
                generate,
                index_files,
                min_tls,
                output,
                preprocess,
                root_dir,
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
            bool {
                accept_timeout,
                cache,
                dump,
                dump_inputs,
                exclude_all_private,
                exclude_link_local,
                exclude_loopback,
                exclude_private,
                glob_ignore_case,
                hidden,
                host_stats,
                include_fragments,
                include_mail,
                include_verbatim,
                include_wikilinks,
                insecure,
                no_ignore,
                no_progress,
                offline,
                require_https,
                skip_missing,
                suggest,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use clap::{CommandFactory, FromArgMatches};
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
        fs::read_to_string("./src/options.rs")
            .expect("Unable to read this source code file to string")
    }

    fn parse_options(args: Vec<&str>) -> LycheeOptions {
        let mut matches = <LycheeOptions as CommandFactory>::command().get_matches_from(args);
        <LycheeOptions as FromArgMatches>::from_arg_matches_mut(&mut matches).unwrap()
    }
}
