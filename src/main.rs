#[macro_use]
extern crate log;

use anyhow::Result;
use futures::future::join_all;
use gumdrop::Options;
use regex::RegexSet;
use reqwest::Url;
use std::{collections::HashSet, env};

mod checker;
mod collector;
mod extract;
mod options;

use checker::{CheckStatus, Checker};
use options::LycheeOptions;

fn main() -> Result<()> {
    pretty_env_logger::init();
    let opts = LycheeOptions::parse_args_default_or_exit();

    let mut runtime = match opts.threads {
        Some(threads) => {
            // We define our own runtime instead of the `tokio::main` attribute since we want to make the number of threads configurable
            tokio::runtime::Builder::new()
                .threaded_scheduler()
                .core_threads(threads)
                .enable_all()
                .build()?
        }
        None => tokio::runtime::Runtime::new()?,
    };
    runtime.block_on(run(opts))?;
    Ok(())
}

fn print_summary(found: &HashSet<Url>, results: &Vec<CheckStatus>) {
    let found = found.len();
    let excluded: usize = results
        .iter()
        .filter(|l| matches!(l, CheckStatus::Excluded))
        .count();
    let success: usize = results
        .iter()
        .filter(|l| matches!(l, CheckStatus::OK))
        .count();
    let errors: usize = found - excluded - success;

    println!("");
    println!("📝Summary");
    println!("-------------------");
    println!("🔍Found: {}", found);
    println!("👻Excluded: {}", excluded);
    println!("✅Successful: {}", success);
    println!("🚫Errors: {}", errors);
}

async fn run(opts: LycheeOptions) -> Result<()> {
    let excludes = RegexSet::new(opts.exclude).unwrap();

    let checker = Checker::try_new(
        env::var("GITHUB_TOKEN")?,
        Some(excludes),
        opts.max_redirects,
        opts.user_agent,
        opts.insecure,
        opts.scheme,
        opts.verbose,
    )?;

    let links = collector::collect_links(opts.inputs).await?;
    let futures: Vec<_> = links.iter().map(|l| checker.check(&l)).collect();
    let results = join_all(futures).await;

    if opts.verbose {
        print_summary(&links, &results);
    }
    let errorcode = if results.iter().all(|r| r.is_success()) {
        0
    } else {
        1
    };
    std::process::exit(errorcode)
}
