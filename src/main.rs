#[macro_use]
extern crate log;

use anyhow::Result;
use std::env;
use std::fs;

mod checker;
mod extract;

use checker::Checker;
use extract::extract_links;
use futures::future::join_all;

use gumdrop::Options;

#[derive(Debug, Options)]
struct LycheeOptions {
    #[options(help = "show help")]
    help: bool,

    #[options(help = "Input file containing the links to check")]
    input: Option<String>,

    #[options(help = "Verbose program output")]
    verbose: bool,

    // Accumulate all exclusions in a vector
    #[options(help = "Exclude URLs from checking (supports regex)")]
    excludes: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();

    let opts = LycheeOptions::parse_args_default_or_exit();

    let checker = Checker::try_new(env::var("GITHUB_TOKEN")?, opts.verbose)?;
    let md = fs::read_to_string(opts.input.unwrap_or_else(|| "README.md".into()))?;
    let links = extract_links(&md);

    let futures: Vec<_> = links.iter().map(|l| checker.check(&l)).collect();
    let results = join_all(futures).await;

    let errorcode = if results.iter().all(|r| r == &true) {
        0
    } else {
        1
    };
    std::process::exit(errorcode)
}
