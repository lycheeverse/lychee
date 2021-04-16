use std::{collections::HashSet, path::PathBuf, str::FromStr, time::Duration};

use anyhow::{anyhow, bail, Result};
use headers::{
    authorization::Basic, Authorization, HeaderMap, HeaderMapExt, HeaderName, HeaderValue,
};
use http::StatusCode;
use lychee_lib::{collector::Input, ClientBuilder};
#[cfg(feature = "config_file")]
use serde::Deserialize;
use structopt::{clap::crate_version, StructOpt};

#[cfg(feature = "config_file")]
mod config_file;

const METHOD: &str = "get";
const USER_AGENT: &str = concat!("lychee/", crate_version!());
const MAX_CONCURRENCY: usize = 128;
const MAX_REDIRECTS: usize = 10;
const TIMEOUT: usize = 20;

// this exists because structopt requires `&str` type values for defaults
// (we can't use e.g. `TIMEOUT` or `timeout()` which gets created for serde)
const MAX_CONCURRENCY_STR: &str = "128";
const MAX_REDIRECTS_STR: &str = "10";
const TIMEOUT_STR: &str = "20";

#[derive(Debug, StructOpt)]
#[structopt(
    name = "lychee",
    about = "A glorious link checker.\n\
             \n\
             Project home page: https://github.com/lycheeverse/lychee"
)]
pub(crate) struct LycheeOptions {
    /// The inputs (where to get links to check from).
    /// These can be: files (e.g. `README.md`), file globs (e.g. `"~/git/*/README.md"`),
    /// remote URLs (e.g. `https://example.org/README.md`) or standard input (`-`).
    /// Prefix with `--` to separate inputs from options that allow multiple arguments.
    // FIXME: replace "README.md" with something that must exists on user's device
    #[structopt(name = "inputs", default_value = "README.md")]
    raw_inputs: Vec<String>,

    /// Configuration file to use
    #[cfg(feature = "config_file")]
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

    #[cfg(not(feature = "config_file"))]
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn load_options() -> Result<Self> {
        let opts = LycheeOptions::from_args();
        Ok(opts)
    }

    #[cfg(feature = "config_file")]
    pub(crate) fn load_options() -> Result<Self> {
        let mut opts = LycheeOptions::from_args();
        // Load a potentially existing config file and merge it into the config from the CLI
        // Requires `serde` feature
        if let Some(c) = Config::load_from_file(&opts.config_file)? {
            opts.config.merge(c);
        }
        Ok(opts)
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, StructOpt)]
#[cfg_attr(feature = "config_file", derive(Deserialize))]
#[cfg_attr(feature = "config_file", serde(default))]
pub(crate) struct Config {
    /// Verbose program output
    #[structopt(short, long)]
    pub(crate) verbose: bool,

    /// Do not show progress bar.
    /// This is recommended for non-interactive shells (e.g. for continuous integration)
    #[cfg(feature = "indicatif")]
    #[structopt(short, long, verbatim_doc_comment)]
    pub(crate) no_progress: bool,

    /// Maximum number of allowed redirects
    #[structopt(short, long, default_value = MAX_REDIRECTS_STR)]
    pub(crate) max_redirects: usize,

    /// Maximum number of concurrent network requests
    #[structopt(long, default_value = MAX_CONCURRENCY_STR)]
    pub(crate) max_concurrency: usize,

    /// Number of threads to utilize.
    /// Defaults to number of cores available to the system
    #[structopt(short = "T", long)]
    pub(crate) threads: Option<usize>,

    /// User agent
    #[structopt(short, long, default_value = USER_AGENT)]
    pub(crate) user_agent: String,

    /// Proceed for server connections considered insecure (invalid TLS)
    #[structopt(short, long)]
    pub(crate) insecure: bool,

    /// Only test links with the given scheme (e.g. https)
    #[structopt(short, long)]
    pub(crate) scheme: Option<String>,

    /// URLs to check (supports regex). Has preference over all excludes.
    #[structopt(long)]
    pub(crate) include: Vec<String>,

    /// Exclude URLs from checking (supports regex)
    #[structopt(long)]
    pub(crate) exclude: Vec<String>,

    /// Exclude all private IPs from checking.
    /// Equivalent to `--exclude-private --exclude-link-local --exclude-loopback`
    #[structopt(short = "E", long, verbatim_doc_comment)]
    pub(crate) exclude_all_private: bool,

    /// Exclude private IP address ranges from checking
    #[structopt(long)]
    pub(crate) exclude_private: bool,

    /// Exclude link-local IP address range from checking
    #[structopt(long)]
    pub(crate) exclude_link_local: bool,

    /// Exclude loopback IP address range from checking
    #[structopt(long)]
    pub(crate) exclude_loopback: bool,

    /// Exclude all mail addresses from checking
    #[structopt(long)]
    pub(crate) exclude_mail: bool,

    /// Custom request headers
    #[structopt(short, long)]
    pub(crate) headers: Vec<String>,

    /// Comma-separated list of accepted status codes for valid links
    #[structopt(short, long)]
    pub(crate) accept: Option<String>,

    /// Website timeout from connect to response finished
    #[structopt(short, long, default_value = TIMEOUT_STR)]
    pub(crate) timeout: usize,

    /// Request method
    // Using `-X` as a short param similar to curl
    #[structopt(short = "X", long, default_value = METHOD)]
    pub(crate) method: String,

    /// Base URL to check relative URLs
    #[structopt(short, long)]
    pub(crate) base_url: Option<String>,

    /// Basic authentication support. E.g. `username:password`
    #[structopt(long)]
    pub(crate) basic_auth: Option<String>,
    /// GitHub API token to use when checking github.com links, to avoid rate limiting
    #[structopt(long, env = "GITHUB_TOKEN")]
    pub(crate) github_token: Option<String>,

    /// Skip missing input files (default is to error if they don't exist)
    #[structopt(long)]
    pub(crate) skip_missing: bool,

    /// Ignore case when expanding filesystem path glob inputs
    #[structopt(long)]
    pub(crate) glob_ignore_case: bool,

    /// Output file of status report
    #[structopt(short, long, parse(from_os_str))]
    pub(crate) output: Option<PathBuf>,

    /// Output file format of status report (json, string)
    #[cfg(feature = "json_output")]
    #[structopt(short, long, default_value = "string")]
    pub(crate) format: crate::format::Format,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            verbose: false,
            #[cfg(feature = "indicatif")]
            no_progress: false,
            max_redirects: MAX_REDIRECTS,
            max_concurrency: MAX_CONCURRENCY,
            threads: None,
            user_agent: USER_AGENT.to_owned(),
            insecure: false,
            scheme: None,
            include: Vec::default(),
            exclude: Vec::default(),
            exclude_all_private: false,
            exclude_private: false,
            exclude_link_local: false,
            exclude_loopback: false,
            exclude_mail: false,
            headers: Vec::default(),
            accept: None,
            timeout: TIMEOUT,
            method: METHOD.to_owned(),
            base_url: None,
            basic_auth: None,
            github_token: None,
            skip_missing: false,
            glob_ignore_case: false,
            output: None,
            #[cfg(feature = "json_output")]
            format: crate::format::Format::default(),
        }
    }
}

impl Config {
    pub(crate) fn build(&self) -> Result<lychee_lib::Client> {
        let headers = self.parse_headers()?;
        let accepted = self.parse_statuscodes()?;
        let timeout = Duration::from_secs(self.timeout as u64);
        let method = reqwest::Method::from_str(&self.method.to_uppercase())?;
        let include = regex::RegexSet::new(&self.include)?;
        let exclude = regex::RegexSet::new(&self.exclude)?;

        ClientBuilder::builder()
            .includes(include)
            .excludes(exclude)
            .exclude_all_private(self.exclude_all_private)
            .exclude_private_ips(self.exclude_private)
            .exclude_link_local_ips(self.exclude_link_local)
            .exclude_loopback_ips(self.exclude_loopback)
            .exclude_mail(self.exclude_mail)
            .max_redirects(self.max_redirects)
            .user_agent(self.user_agent.clone())
            .allow_insecure(self.insecure)
            .custom_headers(headers)
            .method(method)
            .timeout(timeout)
            .github_token(self.github_token.clone())
            .scheme(self.scheme.clone())
            .accepted(accepted)
            .build()
            .client()
            .map_err(|e| anyhow!(e))
    }

    fn read_header(input: &str) -> Result<(&str, &str)> {
        let mut elements = input.split("=");
        if let Some(key) = elements.next() {
            if let Some(value) = elements.next() {
                if elements.next().is_none() {
                    return Ok((key, value));
                }
            }
        }
        bail!(
            "Header value should be of the form key=value, got {}",
            input
        )
    }

    fn parse_headers(&self) -> Result<HeaderMap> {
        let mut headers = self
            .headers
            .iter()
            .map(|header| {
                Self::read_header(header).and_then(
                    |(key, val): (&str, &str)| -> Result<(HeaderName, HeaderValue)> {
                        Ok((
                            HeaderName::from_bytes(key.as_bytes())?,
                            HeaderValue::from_bytes(val.as_bytes())?,
                        ))
                    },
                )
            })
            .collect::<Result<HeaderMap>>()?;
        if let Some(auth_header) = self.parse_basic_auth()? {
            headers.typed_insert(auth_header);
        };
        Ok(headers)
    }

    fn parse_basic_auth(&self) -> Result<Option<Authorization<Basic>>> {
        if let Some(auth) = &self.basic_auth {
            let mut params = auth.split(':');
            if let Some(username) = params.next() {
                if let Some(password) = params.next() {
                    if params.next().is_none() {
                        return Ok(Some(Authorization::basic(username, password)));
                    }
                }
            }
            bail!(
                "Basic auth value should be of the form username:password, got {}",
                auth
            )
        }
        Ok(None)
    }

    fn parse_statuscodes(&self) -> Result<Option<HashSet<StatusCode>>> {
        self.accept
            .as_ref()
            .map(|code: &String| -> Result<HashSet<StatusCode>> {
                code.split(',')
                    .map(|code| StatusCode::from_bytes(code.as_bytes()).map_err(|e| e.into()))
                    .collect()
            })
            .transpose()
    }
}

#[cfg(test)]
mod test {
    use std::{array, collections::HashSet};

    use anyhow::Result;
    use headers::HeaderMap;
    use http::StatusCode;
    use pretty_assertions::assert_eq;
    use reqwest::header;

    use super::Config;

    #[test]
    fn test_parse_custom_headers() -> Result<()> {
        let actual = Config {
            headers: vec!["accept=text/html".to_owned()],
            ..Config::default()
        }
        .parse_headers()?;

        let mut expected = HeaderMap::new();
        expected.insert(header::ACCEPT, "text/html".parse().unwrap());

        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn test_parse_statuscodes() -> Result<()> {
        let actual = Config {
            accept: Some("200,204,301".to_owned()),
            ..Config::default()
        }
        .parse_statuscodes()?
        .unwrap_or_default();

        let expected = array::IntoIter::new([
            StatusCode::OK,
            StatusCode::NO_CONTENT,
            StatusCode::MOVED_PERMANENTLY,
        ])
        .collect::<HashSet<_>>();

        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn test_parse_basic_auth() -> Result<()> {
        let actual = Config {
            basic_auth: Some("aladin:abretesesamo".to_owned()),
            ..Config::default()
        }
        .parse_basic_auth()?
        .unwrap();

        let expected = headers::Authorization::basic("aladin", "abretesesamo");

        assert_eq!(expected, actual);

        Ok(())
    }
}
