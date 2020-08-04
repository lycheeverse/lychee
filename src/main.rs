use github_rs::client::{Executor, Github};
use github_rs::StatusCode;
use pulldown_cmark::{Event, Parser, Tag};
use regex::Regex;
use serde_json::Value;
use std::fs;
use std::{env, error::Error};

struct Checker {
    client: Github,
}

impl Checker {
    /// Creates a new link checker
    pub fn new(token: String) -> Self {
        let client = Github::new(token).unwrap();
        Checker { client }
    }

    fn check_github(&self, owner: String, repo: String) -> bool {
        let (_headers, status, _json) = self
            .client
            .get()
            .repos()
            .owner(&owner)
            .repo(&repo)
            .execute::<Value>()
            .expect("Get failed");
        status == StatusCode::OK
    }

    fn check_normal(&self, url: &str) -> bool {
        let res = reqwest::blocking::get(url);
        if let Ok(res) = res {
            res.status().is_success()
        } else {
            false
        }
    }

    fn extract_github(&self, url: &str) -> Result<(String, String), Box<dyn Error>> {
        let re = Regex::new(r"github\.com/([^/]*)/([^/]*)")?;
        let caps = re.captures(&url).ok_or("Invalid capture")?;
        let owner = caps.get(1).ok_or("Cannot capture owner")?;
        let repo = caps.get(2).ok_or("Cannot capture repo")?;
        Ok((owner.as_str().into(), repo.as_str().into()))
    }

    pub fn check(&self, url: &str) -> bool {
        if self.check_normal(&url) {
            return true;
        }
        // Pull out the heavy weapons in case of a failed normal request.
        // This could be a Github URL and we run into the rate limiter.
        if let Ok((owner, repo)) = self.extract_github(&url) {
            return self.check_github(owner, repo);
        }
        false
    }
}

fn extract_links(md: &str) -> Vec<String> {
    let mut links: Vec<String> = Vec::new();
    Parser::new(md).for_each(|event| match event {
        Event::Start(Tag::Link(_, link, _)) => links.push(link.into_string()),
        Event::Start(Tag::Image(_, link, _)) => links.push(link.into_string()),
        _ => (),
    });

    links
}

struct Args {
    verbose: bool,
    input: Option<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = pico_args::Arguments::from_env();
    let args = Args {
        verbose: args.contains(["-v", "--verbose"]),
        input: args.opt_value_from_str(["-i", "--input"])?,
    };

    let checker = Checker::new(env::var("GITHUB_TOKEN")?);
    let md = fs::read_to_string(args.input.unwrap_or("README.md".into()))?;
    let links: Vec<String> = extract_links(&md);

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

    #[test]
    fn test_is_github() {
        let checker = Checker::new("foo".into());
        assert_eq!(
            checker
                .extract_github("https://github.com/mre/idiomatic-rust")
                .unwrap(),
            ("mre".into(), "idiomatic-rust".into())
        );
    }

    #[test]
    fn test_github() {
        let checker = Checker::new(env::var("GITHUB_TOKEN").unwrap());
        assert_eq!(
            checker.check("https://github.com/mre/idiomatic-rust".into()),
            true
        );
    }

    #[test]
    fn test_github_nonexistent() {
        let checker = Checker::new(env::var("GITHUB_TOKEN").unwrap());
        assert_eq!(
            checker.check("https://github.com/mre/idiomatic-rust-doesnt-exist-man".into()),
            false
        );
    }

    #[test]
    fn test_non_github() {
        let checker = Checker::new(env::var("GITHUB_TOKEN").unwrap());
        let valid = checker.check("https://endler.dev".into());
        assert_eq!(valid, true);
    }

    #[test]
    fn test_non_github_nonexistent() {
        let checker = Checker::new(env::var("GITHUB_TOKEN").unwrap());
        let valid = checker.check("https://endler.dev/abcd".into());
        assert_eq!(valid, false);
    }
}
