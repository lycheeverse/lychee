use anyhow::{Context, Result};
use github_rs::client::{Executor, Github};
use github_rs::StatusCode;
use regex::{Regex, RegexSet};
use reqwest::header::{self, HeaderValue};
use serde_json::Value;
use url::Url;

/// A link checker using an API token for Github links
/// otherwise a normal HTTP client.
pub(crate) struct Checker {
    reqwest_client: reqwest::Client,
    gh_client: Github,
    excludes: RegexSet,
    verbose: bool,
}

impl Checker {
    /// Creates a new link checker
    pub fn try_new(token: String, excludes: RegexSet, verbose: bool) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        // Faking the user agent is necessary for some websites, unfortunately.
        // Otherwise we get a 403 from the firewall (e.g. Sucuri/Cloudproxy on ldra.com).
        headers.insert(header::USER_AGENT, HeaderValue::from_str("curl/7.71.1")?);
        headers.insert(header::TRANSFER_ENCODING, HeaderValue::from_str("chunked")?);

        let reqwest_client = reqwest::ClientBuilder::new()
            .gzip(true)
            .default_headers(headers)
            .build()?;

        let gh_client = Github::new(token).unwrap();
        Ok(Checker {
            reqwest_client,
            gh_client,
            excludes,
            verbose,
        })
    }

    fn check_github(&self, owner: String, repo: String) -> bool {
        info!("Check Github: {}/{}", owner, repo);
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

    async fn check_normal(&self, url: &Url) -> bool {
        let res = self.reqwest_client.get(url.as_str()).send().await;
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

    pub async fn check_real(&self, url: &Url) -> bool {
        if self.check_normal(&url).await {
            return true;
        }
        // Pull out the heavy weapons in case of a failed normal request.
        // This could be a Github URL and we run into the rate limiter.
        if let Ok((owner, repo)) = self.extract_github(url.as_str()) {
            return self.check_github(owner, repo);
        }
        false
    }

    pub async fn check(&self, url: &Url) -> bool {
        // TODO: Indicate that the URL was skipped in the return value.
        // (Perhaps we want to return an enum value here: Status::Skipped)
        if self.excludes.is_match(url.as_str()) {
            return true;
        }

        let ret = self.check_real(&url).await;
        match ret {
            true => {
                if self.verbose {
                    println!("✅{}", &url);
                }
            }
            false => {
                println!("❌{}", &url);
            }
        };
        ret
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::env;
    use url::Url;

    #[test]
    fn test_is_github() {
        let checker = Checker::try_new("foo".into(), false).unwrap();
        assert_eq!(
            checker
                .extract_github("https://github.com/mre/idiomatic-rust")
                .unwrap(),
            ("mre".into(), "idiomatic-rust".into())
        );
    }

    #[tokio::test]
    async fn test_github() {
        let checker = Checker::try_new(env::var("GITHUB_TOKEN").unwrap(), false).unwrap();
        assert_eq!(
            checker
                .check(&Url::parse("https://github.com/mre/idiomatic-rust").unwrap())
                .await,
            true
        );
    }

    #[tokio::test]
    async fn test_github_nonexistent() {
        let checker = Checker::try_new(env::var("GITHUB_TOKEN").unwrap(), false).unwrap();
        assert_eq!(
            checker
                .check(
                    &Url::parse("https://github.com/mre/idiomatic-rust-doesnt-exist-man").unwrap()
                )
                .await,
            false
        );
    }

    #[tokio::test]
    async fn test_non_github() {
        let checker = Checker::try_new(env::var("GITHUB_TOKEN").unwrap(), false).unwrap();
        let valid = checker
            .check(&Url::parse("https://endler.dev").unwrap())
            .await;
        assert_eq!(valid, true);
    }

    #[tokio::test]
    async fn test_non_github_nonexistent() {
        let checker = Checker::try_new(env::var("GITHUB_TOKEN").unwrap(), false).unwrap();
        let valid = checker
            .check(&Url::parse("https://endler.dev/abcd").unwrap())
            .await;
        assert_eq!(valid, false);
    }
}
