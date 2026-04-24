//! `lychee` is a fast, asynchronous, resource-friendly link checker.
//! It is able to find broken hyperlinks and mail addresses in websites
//! and Markdown, HTML, and other file formats.
//!
//! The lychee binary is a wrapper around lychee-lib, which provides
//! convenience functions for calling lychee from the command-line.
//!
//! Run it inside a repository with a `README.md`:
//! ```
//! lychee
//! ```
//!
//! You can also specify various types of inputs:
//!
//! Check links on a website:
//!
//! ```sh
//! lychee https://endler.dev/
//! ```
//!
//! Check links in a remote file:
//! ```sh
//! lychee https://raw.githubusercontent.com/lycheeverse/lychee/master/README.md
//! ```
//!
//! Check links in local file(s):
//! ```sh
//! lychee README.md
//! lychee test.html info.txt
//! ```
//!
//! Check links in local files (by shell glob):
//! ```sh
//! lychee ~/projects/*/README.md
//! ```
//!
//! Check links in local files (lychee supports advanced globbing and `~` expansion):
//! ```sh
//! lychee "~/projects/big_project/**/README.*"
//! ```
//!
//! Ignore case when globbing and check result for each link:
//! ```sh
//! lychee --glob-ignore-case --verbose "~/projects/**/[r]eadme.*"
//! ```
#![warn(clippy::all, clippy::pedantic)]
#![warn(
    absolute_paths_not_starting_with_crate,
    rustdoc::invalid_html_tags,
    missing_copy_implementations,
    missing_debug_implementations,
    semicolon_in_expressions_from_macros,
    unreachable_pub,
    unused_extern_crates,
    variant_size_differences,
    clippy::missing_const_for_fn
)]
#![deny(anonymous_parameters, macro_use_extern_crate)]
#![deny(missing_docs)]

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, ErrorKind, IsTerminal, stdin};
use std::num::NonZeroUsize;

use anyhow::{Context, Error, Result};
use clap::{Parser, crate_version};
use commands::{CommandParams, generate};
use formatters::log::init_logging;
use http::HeaderMap;
use log::{error, info, warn};

use lychee_lib::filter::PathExcludes;

use config::HeaderMapExt;
use ring as _; // required for apple silicon

use lychee_lib::BasicAuthExtractor;
use lychee_lib::Collector;
use lychee_lib::CookieJar;

mod cache;
mod client;
mod commands;
mod config;
mod files_from;
mod formatters;
mod parse;
mod progress;
mod time;
mod verbosity;

use crate::formatters::stats::{OutputStats, ResponseStats, output_statistics};
use crate::{
    cache::Cache,
    config::{Config, LYCHEE_CACHE_FILE, LYCHEE_IGNORE_FILE, LycheeOptions},
    formatters::duration::Duration,
    generate::generate,
};

/// A C-like enum that can be cast to `i32` and used as process exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitCode {
    Success = 0,
    // NOTE: exit code 1 is used for any `Result::Err` bubbled up to `main()`
    // using the `?` operator. For now, 1 acts as a catch-all for everything
    // non-link related (including config errors), until we find a way to
    // structure the error code handling better.
    #[allow(unused)]
    UnexpectedFailure = 1,
    LinkCheckFailure = 2,
    ConfigFile = 3,
}

/// Ignore lines starting with this marker in `.lycheeignore` files
const LYCHEEIGNORE_COMMENT_MARKER: &str = "#";

fn main() -> Result<()> {
    // std::process::exit doesn't guarantee that all destructors will be run,
    // therefore we wrap the main code in another function to ensure that.
    // See: https://doc.rust-lang.org/stable/std/process/fn.exit.html
    // Also see: https://www.youtube.com/watch?v=zQC8T71Y8e4
    let exit_code = run_main()?;
    std::process::exit(exit_code);
}

/// Read lines from file; ignore empty lines
fn read_lines(file: &File) -> Result<Vec<String>> {
    let lines: Vec<_> = BufReader::new(file).lines().collect::<Result<_, _>>()?;
    Ok(lines
        .into_iter()
        .filter(|line| {
            !line.is_empty() && !line.trim_start().starts_with(LYCHEEIGNORE_COMMENT_MARKER)
        })
        .collect())
}

/// Handle the system's `RLIMIT_NOFILE` limit to prevent
/// opening too many file descriptors and encountering runtime errors.
/// (see <https://github.com/lycheeverse/lychee/issues/1248>)
///
/// 1. We try to increase the soft limit.
/// 2. If the soft limit is low we reduce lychee's `max_concurrency` accordingly
fn handle_fd_limits(opts: &mut LycheeOptions) {
    use rlimit::increase_nofile_limit;

    /// Baseline overhead estimate to account for file descriptors
    /// which are opened before lychee actually does any link checking.
    /// Estimate is based on observing `lsof -p $(pgrep lychee)`.
    const BASELINE_OVERHEAD: u64 = 20;

    if let Ok(soft_limit) = increase_nofile_limit(u64::MAX) {
        // The relation between `concurrency` and `soft_limit` is roughly:
        // `soft_limit` ≈ `baseline_overhead` + `concurrency`
        #[expect(
            clippy::cast_possible_truncation,
            reason = "max_concurrency is small in practice"
        )]
        let concurrency = soft_limit.saturating_sub(BASELINE_OVERHEAD) as usize;
        let concurrency = NonZeroUsize::new(concurrency).unwrap_or(NonZeroUsize::MIN);

        let requested_concurrency = opts.config.max_concurrency();
        if requested_concurrency > concurrency {
            warn!(
                "System file descriptor limit is {soft_limit} which is too low for the requested \
                concurrency of {requested_concurrency}. Lowering `max_concurrency` to \
                {concurrency} to prevent 'Too many open files' errors.",
            );
            opts.config.set_max_concurrency(concurrency);
        }
    }
}

/// Merge all provided config options into one.
/// This includes the command-line args,
/// the config file and the secrets file in that order.
fn load_config() -> Result<LycheeOptions> {
    // Merge by precedence, parsed CLI args have highest precedence
    let mut opts = LycheeOptions::parse();

    init_logging(&opts.config.verbose(), &opts.config.mode());

    if opts.config_files.is_empty() {
        // No config files provided via CLI; fall back on trying to load the
        // default config files from the current directory
        if let Some(default_config) = config::loaders::default_config_file()? {
            opts.config = opts.config.merge(default_config);
        }
    } else {
        let configs = opts.config_files.iter().rev(); // reverse so that later args have precedence
        for config_file in configs {
            opts.config = opts.config.merge_file(config_file)?;
        }
    }

    if let Ok(lycheeignore) = File::open(LYCHEE_IGNORE_FILE) {
        opts.config.exclude.append(&mut read_lines(&lycheeignore)?);
    }

    // TODO: Remove this warning and the parameter with 1.0
    if !&opts.config.exclude_file.is_empty() {
        warn!(
            "WARNING: `--exclude-file` is deprecated and will soon be removed; use the `{LYCHEE_IGNORE_FILE}` file to ignore URL patterns instead. To exclude paths of files and directories, use `--exclude-path`."
        );
    }

    // TODO: Remove this warning and the parameter with 1.0
    if opts.config.base.is_some() {
        warn!(
            "WARNING: `--base` is deprecated and will soon be removed; use `--base-url` instead."
        );
    }

    // Load excludes from file
    for path in &opts.config.exclude_file {
        let file = File::open(path)?;
        opts.config.exclude.append(&mut read_lines(&file)?);
    }

    handle_fd_limits(&mut opts);

    Ok(opts)
}

/// Load cookie jar from path (if exists)
fn load_cookie_jar(cfg: &Config) -> Result<Option<CookieJar>> {
    match &cfg.cookie_jar {
        Some(path) => Ok(CookieJar::load(path.clone()).map(Some)?),
        None => Ok(None),
    }
}

/// Load cache (if exists and is still valid)
/// This returns an `Option` as starting without a cache is a common scenario
/// and we silently discard errors on purpose
#[must_use]
fn load_cache(cfg: &Config) -> Option<Cache> {
    if !cfg.cache() {
        return None;
    }

    let max_cache_age = cfg.max_cache_age();

    // Discard entire cache if it hasn't been updated since `max_cache_age`.
    // This is an optimization, which avoids iterating over the file and
    // checking the age of each entry.
    match fs::metadata(LYCHEE_CACHE_FILE) {
        Err(_e) => {
            // No cache found; silently start with empty cache
            return None;
        }
        Ok(metadata) => {
            let modified = metadata.modified().ok()?;
            let elapsed = modified.elapsed().ok()?;
            if elapsed > max_cache_age {
                warn!(
                    "Cache is too old (age: {}, max age: {}). Discarding and recreating.",
                    Duration::from_secs(elapsed.as_secs()),
                    Duration::from_secs(max_cache_age.as_secs())
                );
                return None;
            }
            info!(
                "Cache is recent (age: {}, max age: {}). Using.",
                Duration::from_secs(elapsed.as_secs()),
                Duration::from_secs(max_cache_age.as_secs())
            );
        }
    }

    let cache = Cache::load(
        LYCHEE_CACHE_FILE,
        max_cache_age.as_secs(),
        &cfg.cache_exclude_status(),
    );
    match cache {
        Ok(cache) => Some(cache),
        Err(e) => {
            warn!("Error while loading cache: {e}. Continuing without.");
            None
        }
    }
}

/// Set up runtime and call lychee entrypoint
fn run_main() -> Result<i32> {
    use std::process::exit;

    let opts = match load_config() {
        Ok(opts) => opts,
        Err(e) => {
            error!(
                "Error while loading config: {:?}\n\
                See: https://github.com/lycheeverse/lychee/blob/lychee-v{}/lychee.example.toml",
                e,
                crate_version!()
            );
            exit(ExitCode::ConfigFile as i32);
        }
    };

    // Exit if output path parent directory does not exist
    if let Some(output) = &opts.config.output {
        // parent() returns Some("") for paths with no directory component
        let parent = output.parent().filter(|p| !p.as_os_str().is_empty());
        if let Some(parent) = parent
            && !parent.exists()
        {
            error!(
                "Output path `{}` is not writable: parent directory `{}` does not exist",
                output.display(),
                parent.display()
            );
            exit(ExitCode::UnexpectedFailure as i32);
        }
    }

    if let Some(mode) = opts.config.generate {
        print!("{}", generate(&mode)?);
        exit(ExitCode::Success as i32);
    }

    let runtime = match opts.config.threads {
        Some(threads) => {
            // We define our own runtime instead of the `tokio::main` attribute
            // since we want to make the number of threads configurable
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(threads.get())
                .enable_all()
                .build()?
        }
        None => tokio::runtime::Runtime::new()?,
    };

    match runtime.block_on(run(&opts)) {
        Err(e) if Some(ErrorKind::BrokenPipe) == underlying_io_error_kind(&e) => {
            exit(ExitCode::Success as i32);
        }
        res => res,
    }
}

/// Check if the given error can be traced back to an `io::ErrorKind`
/// This is helpful for troubleshooting the root cause of an error.
/// Code is taken from the anyhow documentation.
fn underlying_io_error_kind(error: &Error) -> Option<io::ErrorKind> {
    for cause in error.chain() {
        if let Some(io_error) = cause.downcast_ref::<io::Error>() {
            return Some(io_error.kind());
        }
    }
    None
}

/// Run lychee on the given inputs
async fn run(opts: &LycheeOptions) -> Result<i32> {
    let inputs = opts.inputs()?;

    // Hide the progress bar only when stdin is the sole input and it is
    // interactive (TTY).
    //
    // We restrict this to the sole-input case because with mixed inputs like
    // `lychee - README.md` the file is processed concurrently with stdin, so
    // the order of completion is non-deterministic. Hiding the bar there would
    // be confusing rather than helpful.
    //
    // When stdin is piped (`cat links.txt | lychee -`), `is_terminal()` returns
    // false, so the progress bar is shown normally.
    let is_stdin_input = inputs.len() == 1
        && inputs
            .iter()
            .any(|input| matches!(input.source, lychee_lib::InputSource::Stdin))
        && stdin().is_terminal();

    // TODO: Remove this section after `--base` got removed with 1.0
    let base = match (opts.config.base.clone(), opts.config.base_url.clone()) {
        (None, base_url) => base_url,
        (base, None) => base,
        (_base, base_url) => {
            warn!(
                "WARNING: Both, `--base` and `--base-url` are set. Using `base-url` and ignoring `--base` (as it's deprecated)."
            );
            base_url
        }
    };

    if opts.config.dump_inputs() {
        let exit_code = commands::dump_inputs(
            inputs,
            opts.config.output.as_ref(),
            &opts.config.exclude_path,
            &opts.config.extensions(),
            !opts.config.hidden(),
            // be aware that "no ignore" means do *not* ignore files
            !opts.config.no_ignore(),
        )
        .await?;

        return Ok(exit_code as i32);
    }

    let cache = load_cache(&opts.config).unwrap_or_default();

    let cookie_jar = load_cookie_jar(&opts.config).with_context(|| {
        format!(
            "Cannot load cookie jar from path `{}`",
            opts.config
                .cookie_jar
                .as_ref()
                .map_or_else(|| "<none>".to_string(), |p| p.display().to_string())
        )
    })?;

    let client = client::create(&opts.config, cookie_jar.as_deref())?;

    let mut collector = Collector::new(opts.config.root_dir.clone(), base.unwrap_or_default())?
        .skip_missing_inputs(opts.config.skip_missing())
        .skip_hidden(!opts.config.hidden())
        // be aware that "no ignore" means do *not* ignore files
        .skip_ignored(!opts.config.no_ignore())
        .include_verbatim(opts.config.include_verbatim())
        .headers(HeaderMap::from_header_pairs(&opts.config.headers())?)
        .excluded_paths(PathExcludes::new(opts.config.exclude_path.clone())?)
        // File a bug if you rely on this envvar! It's going to go away eventually.
        .use_html5ever(std::env::var("LYCHEE_USE_HTML5EVER").is_ok_and(|x| x == "1"))
        .include_wikilinks(opts.config.include_wikilinks())
        .preprocessor(opts.config.preprocess.clone())
        .host_pool(client.host_pool());

    collector = if let Some(ref basic_auth) = opts.config.basic_auth {
        collector.basic_auth_extractor(BasicAuthExtractor::new(basic_auth)?)
    } else {
        collector
    };

    let requests = collector.collect_links_from_file_types(inputs, opts.config.extensions());
    let params = CommandParams {
        client,
        cache,
        requests,
        cfg: opts.config.clone(),
        is_stdin_input,
    };

    let exit_code = if opts.config.dump() {
        commands::dump(params).await?
    } else {
        let (response_stats, cache, exit_code, host_pool) = commands::check(params).await?;
        github_warning(&response_stats, &opts.config);
        redirect_warning(&response_stats, &opts.config);

        let stats = OutputStats {
            response_stats,
            host_stats: opts
                .config
                .host_stats()
                .then_some(host_pool.all_host_stats()),
        };
        output_statistics(stats, &opts.config)?;

        if opts.config.cache() {
            cache.store(LYCHEE_CACHE_FILE)?;
        }

        if let Some(cookie_jar) = cookie_jar.as_ref() {
            info!("Saving cookie jar");
            cookie_jar.save().context("Cannot save cookie jar")?;
        }

        exit_code
    };

    Ok(exit_code as i32)
}

/// Display user-friendly message if there were any issues with GitHub URLs
fn github_warning(stats: &ResponseStats, config: &Config) {
    let github_errors = stats
        .error_map
        .values()
        .flatten()
        .any(|body| body.uri.domain() == Some("github.com"));

    if github_errors && config.github_token.is_none() {
        warn!(
            "There were issues with GitHub URLs. You could try setting a GitHub token and running lychee again.",
        );
    }
}

/// Display user-friendly message if there were any redirects
/// in non-verbose mode.
fn redirect_warning(stats: &ResponseStats, config: &Config) {
    let redirects = stats.redirects;
    if redirects > 0 && config.verbose().log_level() < log::Level::Info {
        let noun = if redirects == 1 {
            "redirect"
        } else {
            "redirects"
        };

        warn!(
            "lychee detected {redirects} {noun}. You might want to consider replacing redirecting URLs with the resolved URLs. Run lychee in verbose mode (-v/--verbose) to see details about the redirections.",
        );
    }
}
