#[macro_use]
extern crate log;

use anyhow::{Context, Result};
use github_rs::client::{Executor, Github};
use github_rs::StatusCode;
use pulldown_cmark::{Event, Parser, Tag};
use regex::Regex;
use reqwest::header::{self, HeaderValue};
use serde_json::Value;
use std::env;
use std::{collections::HashSet, fs};
use url::Url;

struct Checker {
    reqwest_client: reqwest::blocking::Client,
    gh_client: Github,
}

impl Checker {
    /// Creates a new link checker
    pub fn try_new(token: String) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        // Faking the user agent is necessary for some websites, unfortunately.
        // Otherwise we get a 403 from the firewall (e.g. Sucuri/Cloudproxy on ldra.com).
        headers.insert(header::USER_AGENT, HeaderValue::from_str("curl/7.71.1")?);
        headers.insert(header::TRANSFER_ENCODING, HeaderValue::from_str("chunked")?);

        let reqwest_client = reqwest::blocking::ClientBuilder::new()
            .gzip(true)
            .default_headers(headers)
            .build()?;

        let gh_client = Github::new(token).unwrap();
        Ok(Checker {
            reqwest_client,
            gh_client,
        })
    }

    fn check_github(&self, owner: String, repo: String) -> bool {
        let (_headers, status, _json) = self
            .gh_client
            .get()
            .repos()
            .owner(&owner)
            .repo(&repo)
            .execute::<Value>()
            .expect("Get failed");
        status == StatusCode::OK
    }

    fn check_normal(&self, url: &Url) -> bool {
        let res = self.reqwest_client.get(url.as_str()).send();
        if res.is_err() {
            warn!("Cannot send request: {:?}", res);
            return false;
        }
        if let Ok(res) = res {
            if res.status().is_success() {
                true
            } else {
                warn!("Request with non-ok status code: {:?}", res);
                false
            }
        } else {
            warn!("Invalid response: {:?}", res);
            false
        }
    }

    fn extract_github(&self, url: &str) -> Result<(String, String)> {
        let re = Regex::new(r"github\.com/([^/]*)/([^/]*)")?;
        let caps = re.captures(&url).context("Invalid capture")?;
        let owner = caps.get(1).context("Cannot capture owner")?;
        let repo = caps.get(2).context("Cannot capture repo")?;
        Ok((owner.as_str().into(), repo.as_str().into()))
    }

    pub fn check(&self, url: &Url) -> bool {
        if self.check_normal(&url) {
            return true;
        }
        // Pull out the heavy weapons in case of a failed normal request.
        // This could be a Github URL and we run into the rate limiter.
        if let Ok((owner, repo)) = self.extract_github(url.as_str()) {
            return self.check_github(owner, repo);
        }
        false
    }
}

fn extract_links(md: &str) -> HashSet<Url> {
    let mut links: Vec<String> = Vec::new();
    Parser::new(md).for_each(|event| match event {
        Event::Start(Tag::Link(_, link, _)) => links.push(link.into_string()),
        Event::Start(Tag::Image(_, link, _)) => links.push(link.into_string()),
        _ => (),
    });

    // Only keep legit URLs. This sorts out things like anchors.
    // Silently ignore the parse failures for now.
    // TODO: Log errors in verbose mode
    let links: HashSet<Url> = links.iter().flat_map(|l| Url::parse(&l)).collect();
    debug!("Testing links: {:#?}", links);

    links
}

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
