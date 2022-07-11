//! `lychee` is a fast, asynchronous, resource-friendly link checker.
//! It is able to find broken hyperlinks and mail addresses inside Markdown,
//! HTML, `reStructuredText`, and any other format.
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
#![deny(anonymous_parameters, macro_use_extern_crate, pointer_structural_match)]
#![deny(missing_docs)]

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, ErrorKind, Write};
use std::sync::Arc;

use anyhow::{Context, Error, Result};
use color::YELLOW;
use commands::CommandParams;
use formatters::response::ResponseFormatter;
use log::warn;
use openssl_sys as _; // required for vendored-openssl feature
use ring as _; // required for apple silicon
use structopt::StructOpt;

use lychee_lib::Collector;

mod cache;
mod client;
mod color;
mod commands;
mod formatters;
mod options;
mod parse;
mod stats;
mod time;

use crate::{
    cache::{Cache, StoreExt},
    color::color,
    formatters::stats::StatsFormatter,
    options::{Config, Format, LycheeOptions, LYCHEE_CACHE_FILE, LYCHEE_IGNORE_FILE},
};

/// A C-like enum that can be cast to `i32` and used as process exit code.
enum ExitCode {
    Success = 0,
    // NOTE: exit code 1 is used for any `Result::Err` bubbled up to `main()`
    // using the `?` operator. For now, 1 acts as a catch-all for everything
    // non-link related (including config errors), until we find a way to
    // structure the error code handling better.
    #[allow(unused)]
    UnexpectedFailure = 1,
    LinkCheckFailure = 2,
}

/// Ignore lines starting with this marker in `.lycheeignore` files
const LYCHEEINGORE_COMMENT_MARKER: &str = "#";

fn main() -> Result<()> {
    #[cfg(feature = "tokio-console")]
    console_subscriber::init();
    // std::process::exit doesn't guarantee that all destructors will be ran,
    // therefore we wrap "main" code in another function to ensure that.
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
            !line.is_empty() && !line.trim_start().starts_with(LYCHEEINGORE_COMMENT_MARKER)
        })
        .collect())
}

/// Merge all provided config options into one This includes a potential config
/// file, command-line- and environment variables
fn load_config() -> Result<LycheeOptions> {
    let mut opts = LycheeOptions::from_args();

    // Load a potentially existing config file and merge it into the config from
    // the CLI
    if let Some(c) = Config::load_from_file(&opts.config_file)? {
        opts.config.merge(c);
    }

    if let Ok(lycheeignore) = File::open(LYCHEE_IGNORE_FILE) {
        opts.config.exclude.append(&mut read_lines(&lycheeignore)?);
    }

    // TODO: Remove this warning and the parameter in a future release
    if !&opts.config.exclude_file.is_empty() {
        warn!("WARNING: `--exclude-file` is deprecated and will soon be removed; use `{}` file to ignore URL patterns instead. To exclude paths of files and directories, use `--exclude-path`.", LYCHEE_IGNORE_FILE);
    }

    // Load excludes from file
    for path in &opts.config.exclude_file {
        let file = File::open(path)?;
        opts.config.exclude.append(&mut read_lines(&file)?);
    }

    Ok(opts)
}

#[must_use]
/// Load cache (if exists and is still valid)
/// This returns an `Option` as starting without a cache is a common scenario
/// and we silently discard errors on purpose
fn load_cache(cfg: &Config) -> Option<Cache> {
    if !cfg.cache {
        return None;
    }

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
            if elapsed > cfg.max_cache_age {
                eprintln!(
                    "Cache is too old (age: {}, max age: {}). Discarding",
                    humantime::format_duration(elapsed),
                    humantime::format_duration(cfg.max_cache_age)
                );
                return None;
            }
        }
    }

    let cache = Cache::load(LYCHEE_CACHE_FILE, cfg.max_cache_age.as_secs());
    match cache {
        Ok(cache) => Some(cache),
        Err(e) => {
            eprintln!("Error while loading cache: {e}. Continuing without.");
            None
        }
    }
}

/// Set up runtime and call lychee entrypoint
fn run_main() -> Result<i32> {
    use std::process::exit;

    let opts = load_config()?;
    let runtime = match opts.config.threads {
        Some(threads) => {
            // We define our own runtime instead of the `tokio::main` attribute
            // since we want to make the number of threads configurable
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(threads)
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
    let requests = Collector::new(opts.config.base.clone())
        .skip_missing_inputs(opts.config.skip_missing)
        .include_verbatim(opts.config.include_verbatim)
        // File a bug if you rely on this envvar! It's going to go away eventually.
        .use_html5ever(std::env::var("LYCHEE_USE_HTML5EVER").map_or(false, |x| x == "1"))
        .collect_links(inputs)
        .await;
    let client = client::create(&opts.config).await?;
    let cache = load_cache(&opts.config).unwrap_or_default();
    let cache = Arc::new(cache);

    let response_formatter: Box<dyn ResponseFormatter> =
        formatters::get_formatter(&opts.config.format);

    let params = CommandParams {
        client,
        cache,
        requests,
        formatter: response_formatter,
        cfg: opts.config.clone(),
    };

    let exit_code = if opts.config.dump {
        commands::dump(params).await?
    } else {
        let (stats, cache, exit_code) = commands::check(params).await?;

        let github_issues = stats
            .fail_map
            .values()
            .flatten()
            .any(|body| body.uri.domain() == Some("github.com"));

        let writer: Box<dyn StatsFormatter> = match opts.config.format {
            Format::Compact => Box::new(formatters::stats::Compact::new()),
            Format::Detailed => Box::new(formatters::stats::Detailed::new()),
            Format::Json => Box::new(formatters::stats::Json::new()),
            Format::Markdown => Box::new(formatters::stats::Markdown::new()),
            Format::Raw => Box::new(formatters::stats::Raw::new()),
        };
        let is_empty = stats.is_empty();
        let formatted = writer.format_stats(stats)?;

        if let Some(formatted) = formatted {
            if let Some(output) = &opts.config.output {
                fs::write(output, formatted).context("Cannot write status output to file")?;
            } else {
                if opts.config.verbose && !is_empty {
                    // separate summary from the verbose list of links above
                    // with a newline
                    writeln!(io::stdout())?;
                }
                // we assume that the formatted stats don't have a final newline
                writeln!(io::stdout(), "{formatted}")?;
            }
        }

        if github_issues && opts.config.github_token.is_none() {
            let mut f = io::stdout();
            color!(f, YELLOW, "\u{1f4a1} There were issues with Github URLs. You could try setting a Github token and running lychee again.",)?;
        }

        if opts.config.cache {
            cache.store(LYCHEE_CACHE_FILE)?;
        }
        exit_code
    };

    Ok(exit_code as i32)
}
