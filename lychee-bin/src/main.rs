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

// required for apple silicon
use ring as _;
use stats::color_response;

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::iter::FromIterator;
use std::{collections::HashSet, fs, str::FromStr};

use anyhow::{anyhow, Context, Result};
use headers::HeaderMapExt;
use indicatif::{ProgressBar, ProgressStyle};
use lychee_lib::{ClientBuilder, ClientPool, Collector, Input, Request, Response};
use openssl_sys as _; // required for vendored-openssl feature
use regex::RegexSet;
use ring as _; // required for apple silicon
use structopt::StructOpt;
use tokio::sync::mpsc;

mod color;
mod options;
mod parse;
mod stats;
mod writer;

use crate::parse::{parse_basic_auth, parse_headers, parse_statuscodes, parse_timeout};
use crate::{
    options::{Config, Format, LycheeOptions},
    stats::ResponseStats,
    writer::StatsWriter,
};

const LYCHEE_IGNORE_FILE: &str = ".lycheeignore";

/// A C-like enum that can be cast to `i32` and used as process exit code.
enum ExitCode {
    Success = 0,
    // NOTE: exit code 1 is used for any `Result::Err` bubbled up to `main()` using the `?` operator.
    // For now, 1 acts as a catch-all for everything non-link related (including config errors),
    // until we find a way to structure the error code handling better.
    #[allow(unused)]
    UnexpectedFailure = 1,
    LinkCheckFailure = 2,
}

fn main() -> Result<()> {
    #[cfg(feature = "tokio-console")]
    console_subscriber::init();
    // std::process::exit doesn't guarantee that all destructors will be ran,
    // therefore we wrap "main" code in another function to guarantee that.
    // See: https://doc.rust-lang.org/stable/std/process/fn.exit.html
    // Also see: https://www.youtube.com/watch?v=zQC8T71Y8e4
    let exit_code = run_main()?;
    std::process::exit(exit_code);
}

// Read lines from file; ignore empty lines
fn read_lines(file: &File) -> Result<Vec<String>> {
    let lines: Vec<_> = BufReader::new(file).lines().collect::<Result<_, _>>()?;
    Ok(lines.into_iter().filter(|line| !line.is_empty()).collect())
}

fn run_main() -> Result<i32> {
    let mut opts = LycheeOptions::from_args();

    // Load a potentially existing config file and merge it into the config from the CLI
    if let Some(c) = Config::load_from_file(&opts.config_file)? {
        opts.config.merge(c);
    }

    if let Ok(lycheeignore) = File::open(LYCHEE_IGNORE_FILE) {
        opts.config.exclude.append(&mut read_lines(&lycheeignore)?);
    }

    // Load excludes from file
    for path in &opts.config.exclude_file {
        let file = File::open(path)?;
        opts.config.exclude.append(&mut read_lines(&file)?);
    }

    let cfg = &opts.config;

    let runtime = match cfg.threads {
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

    runtime.block_on(run(cfg, opts.inputs()))
}

fn show_progress(progress_bar: &Option<ProgressBar>, response: &Response, verbose: bool) {
    let out = color_response(&response.1);
    if let Some(pb) = progress_bar {
        pb.inc(1);
        pb.set_message(&out);
        if verbose {
            pb.println(out);
        }
    } else {
        if (response.status().is_success() || response.status().is_excluded()) && !verbose {
            return;
        }
        println!("{}", out);
    }
}

async fn run(cfg: &Config, inputs: Vec<Input>) -> Result<i32> {
    let mut headers = parse_headers(&cfg.headers)?;
    if let Some(auth) = &cfg.basic_auth {
        let auth_header = parse_basic_auth(auth)?;
        headers.typed_insert(auth_header);
    }

    let accepted = cfg.accept.clone().and_then(|a| parse_statuscodes(&a).ok());
    let timeout = parse_timeout(cfg.timeout);
    let max_concurrency = cfg.max_concurrency;
    let method: reqwest::Method = reqwest::Method::from_str(&cfg.method.to_uppercase())?;
    let include = RegexSet::new(&cfg.include)?;
    let exclude = RegexSet::new(&cfg.exclude)?;

    // Offline mode overrides the scheme
    let schemes = if cfg.offline {
        vec!["file".to_string()]
    } else {
        cfg.scheme.clone()
    };

    let client = ClientBuilder::builder()
        .includes(include)
        .excludes(exclude)
        .exclude_all_private(cfg.exclude_all_private)
        .exclude_private_ips(cfg.exclude_private)
        .exclude_link_local_ips(cfg.exclude_link_local)
        .exclude_loopback_ips(cfg.exclude_loopback)
        .exclude_mail(cfg.exclude_mail)
        .max_redirects(cfg.max_redirects)
        .user_agent(cfg.user_agent.clone())
        .allow_insecure(cfg.insecure)
        .custom_headers(headers)
        .method(method)
        .timeout(timeout)
        .github_token(cfg.github_token.clone())
        .schemes(HashSet::from_iter(schemes))
        .accepted(accepted)
        .require_https(cfg.require_https)
        .build()
        .client()
        .map_err(|e| anyhow!("Failed to create request client: {}", e))?;

    let links = Collector::new(cfg.base.clone(), cfg.skip_missing, max_concurrency)
        .collect_links(&inputs)
        .await
        .map_err(|e| anyhow!(e))?;

    if cfg.dump {
        let exit_code = dump_links(
            links.iter().filter(|link| !client.filtered(&link.uri)),
            cfg.verbose,
        );
        return Ok(exit_code as i32);
    }

    let pb = if cfg.no_progress {
        None
    } else {
        let bar =
            ProgressBar::new(links.len() as u64).with_style(ProgressStyle::default_bar().template(
                "{spinner:.red.bright} {pos}/{len:.dim} [{elapsed_precise}] {bar:25} {wide_msg}",
            ));
        bar.enable_steady_tick(100);
        Some(bar)
    };

    let (send_req, recv_req) = mpsc::channel(max_concurrency);
    let (send_resp, mut recv_resp) = mpsc::channel(max_concurrency);

    let mut stats = ResponseStats::new();

    let bar = pb.clone();
    tokio::spawn(async move {
        for link in links {
            if let Some(pb) = &bar {
                pb.set_message(&link.to_string());
            };
            send_req.send(link).await.unwrap();
        }
    });

    // Start receiving requests
    tokio::spawn(async move {
        let clients = vec![client; max_concurrency];
        let mut clients = ClientPool::new(send_resp, recv_req, clients);
        clients.listen().await;
    });

    while let Some(response) = recv_resp.recv().await {
        show_progress(&pb, &response, cfg.verbose);
        stats.add(response);
    }

    // Note that print statements may interfere with the progress bar, so this
    // must go before printing the stats
    if let Some(pb) = &pb {
        pb.finish_and_clear();
    }

    let writer: Box<dyn StatsWriter> = match cfg.format {
        Format::Compact => Box::new(writer::Compact::new()),
        Format::Detailed => Box::new(writer::Detailed::new()),
        Format::Json => Box::new(writer::Json::new()),
        Format::Markdown => Box::new(writer::Markdown::new()),
    };

    let code = if stats.is_success() {
        ExitCode::Success
    } else {
        ExitCode::LinkCheckFailure
    };

    write_stats(&*writer, stats, cfg)?;

    Ok(code as i32)
}

/// Dump all detected links to stdout without checking them
fn dump_links<'a>(links: impl Iterator<Item = &'a Request>, verbose: bool) -> ExitCode {
    let mut stdout = io::stdout();
    for link in links {
        // Avoid panic on broken pipe.
        // See https://github.com/rust-lang/rust/issues/46016
        // This can occur when piping the output of lychee
        // to another program like `grep`.

        // Only print source in verbose mode. This way the normal link output
        // can be fed into another tool without data mangling.
        let output = if verbose {
            link.to_string()
        } else {
            link.uri.to_string()
        };
        if let Err(e) = writeln!(stdout, "{}", output) {
            if e.kind() != io::ErrorKind::BrokenPipe {
                eprintln!("{}", e);
                return ExitCode::UnexpectedFailure;
            }
        }
    }
    ExitCode::Success
}

/// Write final statistics to stdout or to file
fn write_stats(writer: &dyn StatsWriter, stats: ResponseStats, cfg: &Config) -> Result<()> {
    let is_empty = stats.is_empty();
    let formatted = writer.write(stats)?;

    if let Some(output) = &cfg.output {
        fs::write(output, formatted).context("Cannot write status output to file")?;
    } else {
        if cfg.verbose && !is_empty {
            // separate summary from the verbose list of links above
            println!();
        }
        // we assume that the formatted stats don't have a final newline
        println!("{}", formatted);
    }
    Ok(())
}
