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
    invalid_html_tags,
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

use std::iter::FromIterator;
use std::{collections::HashSet, fs, str::FromStr, time::Duration};

use anyhow::{anyhow, Context, Result};
use headers::{authorization::Basic, Authorization, HeaderMap, HeaderMapExt, HeaderName};
use http::StatusCode;
use indicatif::{ProgressBar, ProgressStyle};
use lychee_lib::{
    collector::{Collector, Input},
    ClientBuilder, ClientPool, Response,
};
use openssl_sys as _; // required for vendored-openssl feature
use regex::RegexSet;
use ring as _; // required for apple silicon
use structopt::StructOpt;
use tokio::sync::mpsc;

mod options;
mod stats;

use crate::{
    options::{Config, Format, LycheeOptions},
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
    // std::process::exit doesn't guarantee that all destructors will be ran,
    // therefore we wrap "main" code in another function to guarantee that.
    // See: https://doc.rust-lang.org/stable/std/process/fn.exit.html
    // Also see: https://www.youtube.com/watch?v=zQC8T71Y8e4
    let exit_code = run_main()?;
    std::process::exit(exit_code);
}

fn run_main() -> Result<i32> {
    let mut opts = LycheeOptions::from_args();

    // Load a potentially existing config file and merge it into the config from the CLI
    if let Some(c) = Config::load_from_file(&opts.config_file)? {
        opts.config.merge(c)
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

fn fmt(stats: &ResponseStats, format: &Format) -> Result<String> {
    Ok(match format {
        Format::String => stats.to_string(),
        Format::Json => serde_json::to_string_pretty(&stats)?,
    })
}

async fn run(cfg: &Config, inputs: Vec<Input>) -> Result<i32> {
    let mut headers = parse_headers(&cfg.headers)?;
    if let Some(auth) = &cfg.basic_auth {
        let auth_header = parse_basic_auth(&auth)?;
        headers.typed_insert(auth_header);
    }

    let accepted = cfg.accept.clone().and_then(|a| parse_statuscodes(&a).ok());
    let timeout = parse_timeout(cfg.timeout);
    let max_concurrency = cfg.max_concurrency;
    let method: reqwest::Method = reqwest::Method::from_str(&cfg.method.to_uppercase())?;
    let include = RegexSet::new(&cfg.include)?;
    let exclude = RegexSet::new(&cfg.exclude)?;

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
        .schemes(HashSet::from_iter(cfg.scheme.clone()))
        .accepted(accepted)
        .build()
        .client()
        .map_err(|e| anyhow!(e))?;

    let links = Collector::new(cfg.base_url.clone(), cfg.skip_missing, max_concurrency)
        .collect_links(&inputs)
        .await
        .map_err(|e| anyhow!(e))?;

    if cfg.no_check {
        // Printing the banner to stderr and the links to stdout
        // to let the user process the links with shell pipes etc.
        eprintln!("`--no-check` used, dumping links that would be checked:");
        eprintln!("=======================================================");

        for link in links {
            println!("{}", link);
        }
        return Ok(ExitCode::Success as i32);
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

    let stats_formatted = fmt(&stats, &cfg.format)?;
    if let Some(output) = &cfg.output {
        fs::write(output, stats_formatted).context("Cannot write status output to file")?;
    } else {
        if cfg.verbose && !stats.is_empty() {
            // separate summary from the verbose list of links above
            println!();
        }
        // we assume that the formatted stats don't have a final newline
        println!("{}", stats_formatted);
    }

    if stats.is_success() {
        Ok(ExitCode::Success as i32)
    } else {
        Ok(ExitCode::LinkCheckFailure as i32)
    }
}

fn read_header(input: &str) -> Result<(String, String)> {
    let elements: Vec<_> = input.split('=').collect();
    if elements.len() != 2 {
        return Err(anyhow!(
            "Header value should be of the form key=value, got {}",
            input
        ));
    }
    Ok((elements[0].into(), elements[1].into()))
}

const fn parse_timeout(timeout: usize) -> Duration {
    Duration::from_secs(timeout as u64)
}

fn parse_headers<T: AsRef<str>>(headers: &[T]) -> Result<HeaderMap> {
    let mut out = HeaderMap::new();
    for header in headers {
        let (key, val) = read_header(header.as_ref())?;
        out.insert(
            HeaderName::from_bytes(key.as_bytes())?,
            val.parse().unwrap(),
        );
    }
    Ok(out)
}

fn parse_statuscodes<T: AsRef<str>>(accept: T) -> Result<HashSet<StatusCode>> {
    let mut statuscodes = HashSet::new();
    for code in accept.as_ref().split(',') {
        let code: StatusCode = StatusCode::from_bytes(code.as_bytes())?;
        statuscodes.insert(code);
    }
    Ok(statuscodes)
}

fn parse_basic_auth(auth: &str) -> Result<Authorization<Basic>> {
    let params: Vec<_> = auth.split(':').collect();
    if params.len() != 2 {
        return Err(anyhow!(
            "Basic auth value should be of the form username:password, got {}",
            auth
        ));
    }
    Ok(Authorization::basic(params[0], params[1]))
}

#[cfg(test)]
mod test {
    use std::{array, collections::HashSet};

    use headers::{HeaderMap, HeaderMapExt};
    use http::StatusCode;
    use pretty_assertions::assert_eq;
    use reqwest::header;

    use super::{parse_basic_auth, parse_headers, parse_statuscodes};

    #[test]
    fn test_parse_custom_headers() {
        let mut custom = HeaderMap::new();
        custom.insert(header::ACCEPT, "text/html".parse().unwrap());
        assert_eq!(parse_headers(&["accept=text/html"]).unwrap(), custom);
    }

    #[test]
    fn test_parse_statuscodes() {
        let actual = parse_statuscodes("200,204,301").unwrap();
        let expected = array::IntoIter::new([
            StatusCode::OK,
            StatusCode::NO_CONTENT,
            StatusCode::MOVED_PERMANENTLY,
        ])
        .collect::<HashSet<_>>();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_basic_auth() {
        let mut expected = HeaderMap::new();
        expected.insert(
            header::AUTHORIZATION,
            "Basic YWxhZGluOmFicmV0ZXNlc2Ftbw==".parse().unwrap(),
        );

        let mut actual = HeaderMap::new();
        let auth_header = parse_basic_auth("aladin:abretesesamo").unwrap();
        actual.typed_insert(auth_header);

        assert_eq!(expected, actual);
    }
}
