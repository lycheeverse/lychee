use anyhow::{Context, Result};
use github_rs::client::{Executor, Github};
use regex::{Regex, RegexSet};
use reqwest::header::{self, HeaderValue};
use serde_json::Value;
use url::Url;

#[derive(Debug)]
pub enum CheckStatus {
    OK,
    Redirect,
    Excluded,
    Failed(reqwest::StatusCode),
    // github-rs is using an older version of hyper.
    // That's why reqwest::StatusCode and github_rs::StatusCode
    // are incompatible. As a workaround, we add another state for now.
    FailedGithub(github_rs::StatusCode),
    ErrorResponse(reqwest::Error),
}

impl CheckStatus {
    pub fn is_success(&self) -> bool {
        // Probably there's a better way to match here... ;)
        match self {
            CheckStatus::OK => true,
            _ => false,
        }
    }
}

impl From<reqwest::StatusCode> for CheckStatus {
    fn from(s: reqwest::StatusCode) -> Self {
        if s.is_success() {
            CheckStatus::OK
        } else if s.is_redirection() {
            CheckStatus::Redirect
        } else {
            warn!("Request with non-ok status code: {:?}", s);
            CheckStatus::Failed(s)
        }
    }
}

impl From<github_rs::StatusCode> for CheckStatus {
    fn from(s: github_rs::StatusCode) -> Self {
        if s.is_success() {
            CheckStatus::OK
        } else if s.is_redirection() {
            CheckStatus::Redirect
        } else {
            debug!("Request with non-ok status code: {:?}", s);
            CheckStatus::FailedGithub(s)
        }
    }
}

/// A link checker using an API token for Github links
/// otherwise a normal HTTP client.
pub(crate) struct Checker {
    reqwest_client: reqwest::Client,
    gh_client: Github,
    excludes: Option<RegexSet>,
    scheme: Option<String>,
    verbose: bool,
}

impl Checker {
    /// Creates a new link checker
    pub fn try_new(
        token: String,
        excludes: Option<RegexSet>,
        max_redirects: usize,
        user_agent: String,
        allow_insecure: bool,
        scheme: Option<String>,
        verbose: bool,
    ) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        // Faking the user agent is necessary for some websites, unfortunately.
        // Otherwise we get a 403 from the firewall (e.g. Sucuri/Cloudproxy on ldra.com).
        headers.insert(header::USER_AGENT, HeaderValue::from_str(&user_agent)?);
        headers.insert(header::TRANSFER_ENCODING, HeaderValue::from_str("chunked")?);

        let reqwest_client = reqwest::ClientBuilder::new()
            .gzip(true)
            .default_headers(headers)
            .danger_accept_invalid_certs(allow_insecure)
            .redirect(reqwest::redirect::Policy::limited(max_redirects))
            .build()?;

        let scheme = scheme.map(|s| s.to_lowercase());

        let gh_client = Github::new(token).unwrap();
        Ok(Checker {
            reqwest_client,
            gh_client,
            excludes,
            scheme,
            verbose,
        })
    }

    fn check_github(&self, owner: String, repo: String) -> CheckStatus {
        info!("Check Github: {}/{}", owner, repo);
        let (_headers, status, _json) = self
            .gh_client
            .get()
            .repos()
            .owner(&owner)
            .repo(&repo)
            .execute::<Value>()
            .expect("Get failed");
        status.into()
    }

    async fn check_normal(&self, url: &Url) -> CheckStatus {
        let res = self.reqwest_client.get(url.as_str()).send().await;
        match res {
            Ok(response) => response.status().into(),
            Err(e) => {
                warn!("Invalid response: {:?}", e);
                CheckStatus::ErrorResponse(e)
            }
        }
    }

    fn extract_github(&self, url: &str) -> Result<(String, String)> {
        let re = Regex::new(r"github\.com/([^/]*)/([^/]*)")?;
        let caps = re.captures(&url).context("Invalid capture")?;
        let owner = caps.get(1).context("Cannot capture owner")?;
        let repo = caps.get(2).context("Cannot capture repo")?;
        Ok((owner.as_str().into(), repo.as_str().into()))
    }

    pub async fn check_real(&self, url: &Url) -> CheckStatus {
        let status = self.check_normal(&url).await;
        if status.is_success() {
            return status;
        }
        // Pull out the heavy weapons in case of a failed normal request.
        // This could be a Github URL and we run into the rate limiter.
        if let Ok((owner, repo)) = self.extract_github(url.as_str()) {
            return self.check_github(owner, repo);
        }
        status
    }

    fn excluded(&self, url: &Url) -> bool {
        if let Some(excludes) = &self.excludes {
            if excludes.is_match(url.as_str()) {
                return true;
            }
        }
        if Some(url.scheme().to_string()) != self.scheme {
            return true;
        }
        false
    }

    pub async fn check(&self, url: &Url) -> CheckStatus {
        if self.excluded(&url) {
            return CheckStatus::Excluded;
        }

        let ret = self.check_real(&url).await;
        match &ret {
            CheckStatus::OK => {
                if self.verbose {
                    println!("✅{}", &url);
                }
            }
            CheckStatus::Redirect => {
                if self.verbose {
                    println!("🔀️{}", &url);
                }
            }
            CheckStatus::ErrorResponse(e) => {
                println!("🚫{} ({})", &url, e);
            }
            CheckStatus::Failed(e) => {
                println!("🚫{} ({})", &url, e);
            }
            CheckStatus::FailedGithub(e) => {
                println!("🚫{} ({})", &url, e);
            }
            CheckStatus::Excluded => {
                if self.verbose {
                    println!("⏩{}", &url);
                }
            }
        };
        ret
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use url::Url;

    fn get_checker(allow_insecure: bool) -> Checker {
        let checker = Checker::try_new(
            "DUMMY_GITHUB_TOKEN".to_string(),
            None,
            5,
            "curl/7.71.1".to_string(),
            allow_insecure,
            None,
            false,
        )
        .unwrap();
        checker
    }

    #[tokio::test]
    async fn test_nonexistent() {
        let res = get_checker(false)
            .check(&Url::parse("https://endler.dev/abcd").unwrap())
            .await;
        assert!(matches!(res, CheckStatus::Failed(_)));
    }

    #[test]
    fn test_is_github() {
        assert_eq!(
            get_checker(false)
                .extract_github("https://github.com/mre/idiomatic-rust")
                .unwrap(),
            ("mre".into(), "idiomatic-rust".into())
        );
    }
    #[tokio::test]
    async fn test_github() {
        assert!(matches!(
            get_checker(false)
                .check(&Url::parse("https://github.com/mre/idiomatic-rust").unwrap())
                .await,
            CheckStatus::OK
        ));
    }

    #[tokio::test]
    async fn test_github_nonexistent() {
        let res = get_checker(false)
            .check(&Url::parse("https://github.com/mre/idiomatic-rust-doesnt-exist-man").unwrap())
            .await;
        assert!(matches!(res, CheckStatus::FailedGithub(_)));
    }

    #[tokio::test]
    async fn test_non_github() {
        let res = get_checker(false)
            .check(&Url::parse("https://endler.dev").unwrap())
            .await;
        assert!(matches!(res, CheckStatus::OK));
    }

    #[tokio::test]
    async fn test_invalid_ssl() {
        let res = get_checker(false)
            .check(&Url::parse("https://expired.badssl.com/").unwrap())
            .await;
        assert!(matches!(res, CheckStatus::ErrorResponse(_)));

        // Same, but ignore certificate error
        let res = get_checker(true)
            .check(&Url::parse("https://expired.badssl.com/").unwrap())
            .await;
        assert!(matches!(res, CheckStatus::OK));
    }
}
