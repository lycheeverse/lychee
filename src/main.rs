#[macro_use]
extern crate log;

mod checker;
mod extract;

use checker::Checker;
use extract::extract_links;

use anyhow::Result;
use std::env;

use std::fs;

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

    let checker = Checker::try_new(env::var("GITHUB_TOKEN")?)?;
    let md = fs::read_to_string(args.input.unwrap_or("README.md".into()))?;
    let links = extract_links(&md);

    let mut errorcode = 0;
    for link in links {
        match checker.check(&link) {
            true => {
                if args.verbose {
                    println!("✅{}", link);
                }
            }
            false => {
                println!("❌{}", link);
                errorcode = 1;
            }
        }
    }
    std::process::exit(errorcode)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::env;
    use url::Url;

    #[test]
    fn test_is_github() {
        let checker = Checker::try_new("foo".into()).unwrap();
        assert_eq!(
            checker
                .extract_github("https://github.com/mre/idiomatic-rust")
                .unwrap(),
            ("mre".into(), "idiomatic-rust".into())
        );
    }

    #[test]
    fn test_github() {
        let checker = Checker::try_new(env::var("GITHUB_TOKEN").unwrap()).unwrap();
        assert_eq!(
            checker.check(&Url::parse("https://github.com/mre/idiomatic-rust").unwrap()),
            true
        );
    }

    #[test]
    fn test_github_nonexistent() {
        let checker = Checker::try_new(env::var("GITHUB_TOKEN").unwrap()).unwrap();
        assert_eq!(
            checker.check(
                &Url::parse("https://github.com/mre/idiomatic-rust-doesnt-exist-man").unwrap()
            ),
            false
        );
    }

    #[test]
    fn test_non_github() {
        let checker = Checker::try_new(env::var("GITHUB_TOKEN").unwrap()).unwrap();
        let valid = checker.check(&Url::parse("https://endler.dev").unwrap());
        assert_eq!(valid, true);
    }

    #[test]
    fn test_non_github_nonexistent() {
        let checker = Checker::try_new(env::var("GITHUB_TOKEN").unwrap()).unwrap();
        let valid = checker.check(&Url::parse("https://endler.dev/abcd").unwrap());
        assert_eq!(valid, false);
    }
}
