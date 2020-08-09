#[macro_use]
extern crate log;

use anyhow::Result;
use std::env;
use std::fs;

mod checker;
mod extract;

use checker::Checker;
use extract::extract_links;

struct Args {
    verbose: bool,
    input: Option<String>,
}

fn main() -> Result<()> {
    pretty_env_logger::init();

    let mut args = pico_args::Arguments::from_env();
    let args = Args {
        verbose: args.contains(["-v", "--verbose"]),
        input: args.opt_value_from_str(["-i", "--input"])?,
    };

    let checker = Checker::try_new(env::var("GITHUB_TOKEN")?, args.verbose)?;
    let md = fs::read_to_string(args.input.unwrap_or_else(|| "README.md".into()))?;
    let links = extract_links(&md);

    let results: Vec<bool> = links.iter().map(|l| checker.check(&l)).collect();

    let errorcode = if results.iter().all(|r| r == &true) {
        0
    } else {
        1
    };
    std::process::exit(errorcode)
}
