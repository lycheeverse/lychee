use anyhow::anyhow;
use anyhow::{Context, Result};
use hubcaps::{Credentials, Github};
use regex::{Regex, RegexSet};
use reqwest::header::{self, HeaderMap, HeaderValue};
use std::{collections::HashSet, convert::TryFrom, time::Duration};
use url::Url;

pub(crate) enum RequestMethod {
    GET,
    HEAD,
}

impl TryFrom<String> for RequestMethod {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_ref() {
            "get" => Ok(RequestMethod::GET),
            "head" => Ok(RequestMethod::HEAD),
            _ => Err(anyhow!("Only `get` and `head` allowed, got {}", value)),
        }
    }
}

#[derive(Debug)]
pub enum Status {
    Ok(http::StatusCode),
    Failed(http::StatusCode),
    Timeout,
    Redirected,
    Excluded,
    Error(String),
}

impl Status {
    pub fn new(statuscode: http::StatusCode, accepted: Option<HashSet<http::StatusCode>>) -> Self {
        if let Some(accepted) = accepted {
            if accepted.contains(&statuscode) {
                return Status::Ok(statuscode);
            }
        } else {
            if statuscode.is_success() {
                return Status::Ok(statuscode);
            }
        };
        if statuscode.is_redirection() {
            Status::Redirected
        } else {
            Status::Failed(statuscode)
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Status::Ok(_))
    }
}

impl From<reqwest::Error> for Status {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Status::Timeout
        } else {
            Status::Error(e.to_string())
        }
    }
}

/// A link checker using an API token for Github links
/// otherwise a normal HTTP client.
pub(crate) struct Checker {
    reqwest_client: reqwest::Client,
    github: Github,
    excludes: Option<RegexSet>,
    scheme: Option<String>,
    method: RequestMethod,
    accepted: Option<HashSet<reqwest::StatusCode>>,
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
        custom_headers: HeaderMap,
        method: RequestMethod,
        accepted: Option<HashSet<http::StatusCode>>,
        connect_timeout: Option<Duration>,
        verbose: bool,
    ) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        // Faking the user agent is necessary for some websites, unfortunately.
        // Otherwise we get a 403 from the firewall (e.g. Sucuri/Cloudproxy on ldra.com).
        headers.insert(header::USER_AGENT, HeaderValue::from_str(&user_agent)?);
        headers.insert(header::TRANSFER_ENCODING, HeaderValue::from_str("chunked")?);

        headers.extend(custom_headers);

        let builder = reqwest::ClientBuilder::new()
            .gzip(true)
            .default_headers(headers)
            .danger_accept_invalid_certs(allow_insecure)
            .redirect(reqwest::redirect::Policy::limited(max_redirects));

        let builder = match connect_timeout {
            Some(connect_timeout) => builder.connect_timeout(connect_timeout),
            None => builder,
        };

        let reqwest_client = builder.build()?;

        let github = Github::new(user_agent, Credentials::Token(token))?;

        let scheme = scheme.map(|s| s.to_lowercase());

        Ok(Checker {
            reqwest_client,
            github,
            excludes,
            scheme,
            method,
            accepted,
            verbose,
        })
    }

    async fn check_github(&self, owner: String, repo: String) -> Status {
        info!("Check Github: {}/{}", owner, repo);
        let repo = self.github.repo(owner, repo).get().await;
        match repo {
            Err(e) => Status::Error(format!("{}", e)),
            Ok(_) => Status::Ok(http::StatusCode::OK),
        }
    }

    async fn check_normal(&self, url: &Url) -> Status {
        let request = match self.method {
            RequestMethod::GET => self.reqwest_client.get(url.as_str()),
            RequestMethod::HEAD => self.reqwest_client.head(url.as_str()),
        };
        let res = request.send().await;
        match res {
            Ok(response) => Status::new(response.status(), self.accepted.clone()),
            Err(e) => {
                warn!("Invalid response: {:?}", e);
                e.into()
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

    pub async fn check_real(&self, url: &Url) -> Status {
        let status = self.check_normal(&url).await;
        if status.is_success() {
            return status;
        }
        // Pull out the heavy weapons in case of a failed normal request.
        // This could be a Github URL and we run into the rate limiter.
        if let Ok((owner, repo)) = self.extract_github(url.as_str()) {
            return self.check_github(owner, repo).await;
        }
        status
    }

    fn excluded(&self, url: &Url) -> bool {
        if let Some(excludes) = &self.excludes {
            if excludes.is_match(url.as_str()) {
                return true;
            }
        }
        if let Some(scheme) = &self.scheme {
            if url.scheme() != scheme {
                return true;
            }
        }
        false
    }

    pub async fn check(&self, url: &Url) -> Status {
        if self.excluded(&url) {
            return Status::Excluded;
        }

        let ret = self.check_real(&url).await;
        match &ret {
            Status::Ok(code) => {
                if self.verbose {
                    println!("✅{} [{}]", url, code);
                }
            }
            Status::Failed(code) => {
                println!("🚫{} [{}]", url, code);
            }
            Status::Redirected => {
                if self.verbose {
                    println!("🔀️{}", url);
                }
            }
            Status::Excluded => {
                if self.verbose {
                    println!("👻{}", url);
                }
            }
            Status::Error(e) => {
                println!("⚡ {} ({})", url, e);
            }
            Status::Timeout => {
                println!("⌛{}", url);
            }
        };
        ret
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use http::StatusCode;
    use std::time::Duration;
    use url::Url;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn get_checker(allow_insecure: bool, custom_headers: HeaderMap) -> Checker {
        let checker = Checker::try_new(
            "DUMMY_GITHUB_TOKEN".to_string(),
            None,
            5,
            "curl/7.71.1".to_string(),
            allow_insecure,
            None,
            custom_headers,
            RequestMethod::GET,
            None,
            None,
            false,
        )
        .unwrap();
        checker
    }

    #[tokio::test]
    async fn test_nonexistent() {
        let res = get_checker(false, HeaderMap::new())
            .check(&Url::parse("https://endler.dev/abcd").unwrap())
            .await;
        assert!(matches!(res, Status::Failed(_)));
    }

    #[test]
    fn test_is_github() {
        assert_eq!(
            get_checker(false, HeaderMap::new())
                .extract_github("https://github.com/mre/idiomatic-rust")
                .unwrap(),
            ("mre".into(), "idiomatic-rust".into())
        );
    }
    #[tokio::test]
    async fn test_github() {
        assert!(matches!(
            get_checker(false, HeaderMap::new())
                .check(&Url::parse("https://github.com/mre/idiomatic-rust").unwrap())
                .await,
            Status::Ok(_)
        ));
    }

    #[tokio::test]
    async fn test_github_nonexistent() {
        let res = get_checker(false, HeaderMap::new())
            .check(&Url::parse("https://github.com/mre/idiomatic-rust-doesnt-exist-man").unwrap())
            .await;
        assert!(matches!(res, Status::Error(_)));
    }

    #[tokio::test]
    async fn test_non_github() {
        let res = get_checker(false, HeaderMap::new())
            .check(&Url::parse("https://endler.dev").unwrap())
            .await;
        assert!(matches!(res, Status::Ok(_)));
    }

    #[tokio::test]
    async fn test_invalid_ssl() {
        let res = get_checker(false, HeaderMap::new())
            .check(&Url::parse("https://expired.badssl.com/").unwrap())
            .await;
        assert!(matches!(res, Status::Error(_)));

        // Same, but ignore certificate error
        let res = get_checker(true, HeaderMap::new())
            .check(&Url::parse("https://expired.badssl.com/").unwrap())
            .await;
        assert!(matches!(res, Status::Ok(_)));
    }

    #[tokio::test]
    async fn test_custom_headers() {
        let res = get_checker(false, HeaderMap::new())
            .check(&Url::parse("https://crates.io/keywords/cassandra").unwrap())
            .await;
        assert!(matches!(res, Status::Failed(StatusCode::NOT_FOUND)));

        // Try again, but with a custom header.
        // For example, crates.io requires a custom accept header.
        // See https://github.com/rust-lang/crates.io/issues/788
        let mut custom = HeaderMap::new();
        custom.insert(header::ACCEPT, "text/html".parse().unwrap());
        let res = get_checker(true, custom)
            .check(&Url::parse("https://crates.io/keywords/cassandra").unwrap())
            .await;
        assert!(matches!(res, Status::Ok(_)));
    }

    #[tokio::test]
    #[ignore]
    // See https://github.com/LukeMathWalker/wiremock-rs/issues/19
    async fn test_timeout() {
        let mock_server = MockServer::start().await;
        let delay = Duration::from_secs(30);
        let template = ResponseTemplate::new(200).set_delay(delay.clone());
        Mock::given(method("GET"))
            .respond_with(template)
            .mount(&mock_server)
            .await;

        let res = get_checker(false, HeaderMap::new())
            .check(&Url::parse(&mock_server.uri()).unwrap())
            .await;
        println!("{:?}", res);
        assert!(matches!(res, Status::Timeout));
    }
}
