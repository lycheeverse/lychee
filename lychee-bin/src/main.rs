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

use lychee_lib::{Collector, Input, Request, Response};
// required for apple silicon
use ring as _;
use tokio_stream::wrappers::ReceiverStream;

use std::io::{self, Write};

use anyhow::{anyhow, Result};
use futures::stream::TryStreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use openssl_sys as _; // required for vendored-openssl feature
use ring as _; // required for apple silicon
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

mod client;
mod options;
mod parse;
mod stats;

use crate::{
    options::Config,
    stats::{color_response, ResponseStats},
};

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
    // therefore we wrap "main" code in another function to ensure that.
    // See: https://doc.rust-lang.org/stable/std/process/fn.exit.html
    // Also see: https://www.youtube.com/watch?v=zQC8T71Y8e4
    let exit_code = run_main()?;
    std::process::exit(exit_code);
}

fn run_main() -> Result<i32> {
    let merged = options::merge()?;

    let runtime = match merged.config.threads {
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

    runtime.block_on(run(&merged.config, merged.inputs()))
}

async fn run(cfg: &Config, inputs: Vec<Input>) -> Result<i32> {
    let client = client::create(cfg)?;

    println!("Collect links");
    let links = Collector::new(cfg.base.clone(), cfg.skip_missing, cfg.max_concurrency)
        .collect_links(&inputs)
        .await
        .map_err(|e| anyhow!("Cannot collect links from inputs: {}", e));

    let exit_code = if cfg.dump {
        dump_links(
            links
                .collect::<Result<Vec<_>>>()
                .await?
                .iter()
                .filter(|link| !client.filtered(&link.uri)),
        )
    } else {
        let (send_req, recv_req) = mpsc::channel(cfg.max_concurrency);
        let (send_resp, mut recv_resp) = mpsc::channel(cfg.max_concurrency);
        let max_concurrency = cfg.max_concurrency;
        let mut stats = ResponseStats::new();

        // Start receiving requests
        tokio::spawn(async move {
            println!("Spawn checker");
            futures::StreamExt::for_each_concurrent(
                ReceiverStream::new(recv_req),
                max_concurrency,
                |req| async {
                    let resp = client.check(req).await.unwrap();
                    send_resp.send(resp).await.unwrap();
                },
            )
            .await;
        });

        let pb = if cfg.no_progress {
            None
        } else {
            let bar = ProgressBar::new_spinner().with_style(ProgressStyle::default_bar().template(
                "{spinner:.red.bright} {pos}/{len:.dim} [{elapsed_precise}] {bar:25} {wide_msg}",
            ));
            bar.set_length(0);
            bar.set_message("Extracting links");
            bar.enable_steady_tick(100);
            Some(bar)
        };

        let bar = pb.clone();
        let show_results_task = tokio::spawn({
            let verbose = cfg.verbose;
            async move {
                while let Some(response) = recv_resp.recv().await {
                    show_progress(&pb, &response, verbose);
                    stats.add(response);
                }
                (pb, stats)
            }
        });

        tokio::pin!(links);

        while let Some(link) = links.next().await {
            let link = link?;
            if let Some(pb) = &bar {
                pb.inc_length(1);
                pb.set_message(&link.to_string());
            };
            send_req.send(link).await.unwrap();
        }
        // required for the receiver task to end, which closes send_resp, which allows
        // the show_results_task to finish
        drop(send_req);

        let (pb, stats) = show_results_task.await?;

        // Note that print statements may interfere with the progress bar, so this
        // must go before printing the stats
        if let Some(pb) = &pb {
            pb.finish_and_clear();
        }

        stats::write(&stats, cfg)?;

        if stats.is_success() {
            ExitCode::Success
        } else {
            ExitCode::LinkCheckFailure
        }
    };
    Ok(exit_code as i32)
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

/// Dump all detected links to stdout without checking them
fn dump_links<'a>(links: impl Iterator<Item = &'a Request>) -> ExitCode {
    println!("inside dump");
    let mut stdout = io::stdout();
    for link in links {
        // Avoid panic on broken pipe.
        // See https://github.com/rust-lang/rust/issues/46016
        // This can occur when piping the output of lychee
        // to another program like `grep`.
        if let Err(e) = writeln!(stdout, "{}", &link) {
            if e.kind() != io::ErrorKind::BrokenPipe {
                eprintln!("{}", e);
                return ExitCode::UnexpectedFailure;
            }
        }
    }
    ExitCode::Success
}
