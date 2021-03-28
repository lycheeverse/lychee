use anyhow::{anyhow, bail, Context, Result};
use check_if_email_exists::{check_email, CheckEmailInput};
use derive_builder::Builder;
use headers::{HeaderMap, HeaderValue};
use hubcaps::{Credentials, Github};
use regex::{Regex, RegexSet};
use reqwest::header;
use std::convert::TryInto;
use std::{collections::HashSet, time::Duration};
use tokio::time::sleep;
use url::Url;

use crate::filter::Excludes;
use crate::filter::Filter;
use crate::filter::Includes;
use crate::types::{Response, Status};
use crate::uri::Uri;
use crate::Request;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_MAX_REDIRECTS: usize = 5;

#[derive(Debug, Clone)]
pub struct Client {
    reqwest_client: reqwest::Client,
    github: Option<Github>,
    filter: Filter,
    method: reqwest::Method,
    accepted: Option<HashSet<reqwest::StatusCode>>,
}

/// A link checker using an API token for Github links
/// otherwise a normal HTTP client.
#[derive(Builder, Debug)]
#[builder(build_fn(skip))]
#[builder(setter(into))]
#[builder(name = "ClientBuilder")]
pub struct ClientBuilderInternal {
    /// Set an optional Github token.
    /// This allows for more requests before
    /// getting rate-limited.
    github_token: Option<String>,
    /// Check links matching this set of regular expressions
    includes: Option<RegexSet>,
    /// Exclude links matching this set of regular expressions
    excludes: Option<RegexSet>,
    /// Exclude all private network addresses
    exclude_all_private: bool,
    /// Exclude private IP addresses
    exclude_private_ips: bool,
    /// Exclude link-local IPs
    exclude_link_local_ips: bool,
    /// Exclude loopback IP addresses (e.g. 127.0.0.1)
    exclude_loopback_ips: bool,
    /// Don't check mail addresses
    exclude_mail: bool,
    /// Maximum number of redirects before returning error
    max_redirects: usize,
    /// User agent used for checking links
    user_agent: String,
    /// Ignore SSL errors
    allow_insecure: bool,
    /// Allowed URI scheme (e.g. https, http).
    /// This excludes all links from checking, which
    /// don't specify that scheme in the URL.
    scheme: Option<String>,
    /// Map of headers to send to each resource.
    /// This allows working around validation issues
    /// on some websites.
    custom_headers: HeaderMap,
    /// Request method (e.g. `GET` or `HEAD`)
    method: reqwest::Method,
    /// Set of accepted return codes / status codes
    accepted: Option<HashSet<http::StatusCode>>,
    /// Response timeout per request
    timeout: Option<Duration>,
}

impl ClientBuilder {
    fn build_excludes(&mut self) -> Excludes {
        // exclude_all_private option turns on all "private" excludes,
        // including private IPs, link-local IPs and loopback IPs
        let enable_exclude = |opt| opt || self.exclude_all_private.unwrap_or_default();

        Excludes {
            regex: self.excludes.clone().unwrap_or_default(),
            private_ips: enable_exclude(self.exclude_private_ips.unwrap_or_default()),
            link_local_ips: enable_exclude(self.exclude_link_local_ips.unwrap_or_default()),
            loopback_ips: enable_exclude(self.exclude_loopback_ips.unwrap_or_default()),
            mail: enable_exclude(self.exclude_mail.unwrap_or_default()),
        }
    }

    fn build_includes(&mut self) -> Includes {
        Includes {
            regex: self.includes.clone().unwrap_or_default(),
        }
    }

    /// The build method instantiates the client.
    pub fn build(&mut self) -> Result<Client> {
        let mut headers = HeaderMap::new();

        // Faking the user agent is necessary for some websites, unfortunately.
        // Otherwise we get a 403 from the firewall (e.g. Sucuri/Cloudproxy on ldra.com).
        let user_agent = self
            .user_agent
            .clone()
            .unwrap_or_else(|| format!("lychee/{}", VERSION));

        headers.insert(header::USER_AGENT, HeaderValue::from_str(&user_agent)?);
        headers.insert(header::TRANSFER_ENCODING, HeaderValue::from_str("chunked")?);
        if let Some(custom) = &self.custom_headers {
            headers.extend(custom.clone());
        }

        let allow_insecure = self.allow_insecure.unwrap_or(false);
        let max_redirects = self.max_redirects.unwrap_or(DEFAULT_MAX_REDIRECTS);

        let builder = reqwest::ClientBuilder::new()
            .gzip(true)
            .default_headers(headers)
            .danger_accept_invalid_certs(allow_insecure)
            .redirect(reqwest::redirect::Policy::limited(max_redirects));

        let builder = match self.timeout {
            Some(t) => builder
                .timeout(t.ok_or_else(|| anyhow!("cannot parse timeout: {:?}", self.timeout))?),
            None => builder,
        };

        let reqwest_client = builder.build()?;

        let token: Option<String> = self.github_token.clone().unwrap_or_default();
        let github = match token {
            Some(token) => {
                if token.is_empty() {
                    None
                } else {
                    let github = Github::new(user_agent, Credentials::Token(token))?;
                    Some(github)
                }
            }
            None => None,
        };

        let scheme = self.scheme.clone().unwrap_or(None);
        let scheme = scheme.map(|s| s.to_lowercase());

        let includes = self.build_includes();
        let excludes = self.build_excludes();

        let filter = Filter::new(Some(includes), Some(excludes), scheme);

        Ok(Client {
            reqwest_client,
            github,
            filter,
            method: self.method.clone().unwrap_or(reqwest::Method::GET),
            accepted: self.accepted.clone().unwrap_or(None),
        })
    }
}

impl Client {
    pub async fn check<T: TryInto<Request>>(&self, request: T) -> Result<Response> {
        let request: Request = match request.try_into() {
            Ok(request) => request,
            Err(_e) => bail!("Invalid URI"),
        };
        if self.filter.excluded(&request) {
            return Ok(Response::new(request.uri, Status::Excluded, request.source));
        }
        let status = match request.uri {
            Uri::Website(ref url) => self.check_website(&url).await,
            Uri::Mail(ref address) => {
                // TODO: We should not be using a HTTP status code for mail
                match self.valid_mail(&address).await {
                    true => Status::Ok(http::StatusCode::OK),
                    false => Status::Error(format!("Invalid mail address: {}", address), None),
                }
            }
        };
        Ok(Response::new(request.uri, status, request.source))
    }

    pub async fn check_website(&self, url: &Url) -> Status {
        let mut retries: i64 = 3;
        let mut wait: u64 = 1;
        let status = loop {
            let res = self.check_default(&url).await;
            match res.is_success() {
                true => return res,
                false => {
                    if retries > 0 {
                        retries -= 1;
                        sleep(Duration::from_secs(wait)).await;
                        wait *= 2;
                    } else {
                        break res;
                    }
                }
            }
        };
        // Pull out the heavy weapons in case of a failed normal request.
        // This could be a Github URL and we run into the rate limiter.
        if let Ok((owner, repo)) = self.extract_github(url.as_str()) {
            return self.check_github(owner, repo).await;
        }

        status
    }

    async fn check_github(&self, owner: String, repo: String) -> Status {
        match &self.github {
            Some(github) => {
                let repo = github.repo(owner, repo).get().await;
                match repo {
                    Err(e) => Status::Error(e.to_string(), None),
                    Ok(_) => Status::Ok(http::StatusCode::OK),
                }
            }
            None => Status::Error(
                "GitHub token not specified. To check GitHub links reliably, \
                use `--github-token` flag / `GITHUB_TOKEN` env var."
                    .to_string(),
                None,
            ),
        }
    }

    async fn check_default(&self, url: &Url) -> Status {
        let request = self
            .reqwest_client
            .request(self.method.clone(), url.as_str());
        let res = request.send().await;
        match res {
            Ok(response) => Status::new(response.status(), self.accepted.clone()),
            Err(e) => e.into(),
        }
    }

    fn extract_github(&self, url: &str) -> Result<(String, String)> {
        let re = Regex::new(r"github\.com/([^/]*)/([^/]*)")?;
        let caps = re.captures(&url).context("Invalid capture")?;
        let owner = caps.get(1).context("Cannot capture owner")?;
        let repo = caps.get(2).context("Cannot capture repo")?;
        Ok((owner.as_str().into(), repo.as_str().into()))
    }

    pub async fn valid_mail(&self, address: &str) -> bool {
        let input = CheckEmailInput::new(vec![address.to_string()]);
        let results = check_email(&input).await;
        let result = results.get(0);
        match result {
            None => false,
            Some(result) => {
                // Accept everything that is not invalid
                !matches!(
                    result.is_reachable,
                    check_if_email_exists::Reachable::Invalid
                )
            }
        }
    }
}

/// A convenience function to check a single URI
/// This is the most simple link check and avoids having to create a client manually.
/// For more complex scenarios, look into using the `ClientBuilder` instead.
pub async fn check<T: TryInto<Request>>(request: T) -> Result<Response> {
    let client = ClientBuilder::default().build()?;
    Ok(client.check(request).await?)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::time::{Duration, Instant};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_nonexistent() {
        let template = ResponseTemplate::new(404);
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(template)
            .mount(&mock_server)
            .await;

        let res = ClientBuilder::default()
            .build()
            .unwrap()
            .check(mock_server.uri())
            .await
            .unwrap();
        assert!(res.status.is_failure());
    }

    #[tokio::test]
    async fn test_nonexistent_with_path() {
        let res = ClientBuilder::default()
            .build()
            .unwrap()
            .check("http://127.0.0.1/invalid")
            .await
            .unwrap();
        assert!(res.status.is_failure());
    }

    #[tokio::test]
    async fn test_exponential_backoff() {
        let template = ResponseTemplate::new(404);
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(template)
            .mount(&mock_server)
            .await;

        let start = Instant::now();
        let res = ClientBuilder::default()
            .build()
            .unwrap()
            .check(mock_server.uri())
            .await
            .unwrap();
        let end = start.elapsed();

        assert!(matches!(res.status, Status::Error(_, _)));

        // on slow connections, this might take a bit longer than nominal backed-off timeout (7 secs)
        assert!(end.as_secs() >= 7);
        assert!(end.as_secs() <= 8);
    }

    #[test]
    fn test_is_github() {
        assert_eq!(
            ClientBuilder::default()
                .build()
                .unwrap()
                .extract_github("https://github.com/lycheeverse/lychee")
                .unwrap(),
            ("lycheeverse".into(), "lychee".into())
        );
    }
    #[tokio::test]
    async fn test_github() {
        assert!(ClientBuilder::default()
            .build()
            .unwrap()
            .check("https://github.com/lycheeverse/lychee")
            .await
            .unwrap()
            .status
            .is_success());
    }

    #[tokio::test]
    async fn test_github_nonexistent() {
        let res = ClientBuilder::default()
            .build()
            .unwrap()
            .check("https://github.com/lycheeverse/not-lychee")
            .await
            .unwrap()
            .status;
        assert!(res.is_failure());
    }

    #[tokio::test]
    async fn test_non_github() {
        let template = ResponseTemplate::new(200);
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(template)
            .mount(&mock_server)
            .await;

        let res = ClientBuilder::default()
            .build()
            .unwrap()
            .check(mock_server.uri())
            .await
            .unwrap()
            .status;
        assert!(res.is_success());
    }

    #[tokio::test]
    async fn test_invalid_ssl() {
        let res = ClientBuilder::default()
            .build()
            .unwrap()
            .check("https://expired.badssl.com/")
            .await
            .unwrap();
        assert!(res.status.is_failure());

        // Same, but ignore certificate error
        let res = ClientBuilder::default()
            .allow_insecure(true)
            .build()
            .unwrap()
            .check("https://expired.badssl.com/")
            .await
            .unwrap();
        assert!(res.status.is_success());
    }

    #[tokio::test]
    async fn test_custom_headers() {
        let res = ClientBuilder::default()
            .build()
            .unwrap()
            .check("https://crates.io/crates/lychee")
            .await
            .unwrap();
        assert!(res.status.is_failure());

        // Try again, but with a custom header.
        // For example, crates.io requires a custom accept header.
        // See https://github.com/rust-lang/crates.io/issues/788
        let mut custom = HeaderMap::new();
        custom.insert(header::ACCEPT, "text/html".parse().unwrap());
        let res = ClientBuilder::default()
            .custom_headers(custom)
            .build()
            .unwrap()
            .check("https://crates.io/crates/lychee")
            .await
            .unwrap();
        assert!(res.status.is_success());
    }

    #[tokio::test]
    async fn test_timeout() {
        // Note: this checks response timeout, not connect timeout.
        // To check connect timeout, we'd have to do something more involved,
        // see: https://github.com/LukeMathWalker/wiremock-rs/issues/19
        let mock_delay = Duration::from_millis(20);
        let checker_timeout = Duration::from_millis(10);
        assert!(mock_delay > checker_timeout);

        let template = ResponseTemplate::new(200).set_delay(mock_delay);
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(template)
            .mount(&mock_server)
            .await;

        let client = ClientBuilder::default()
            .timeout(checker_timeout)
            .build()
            .unwrap();

        let resp = client.check(mock_server.uri()).await.unwrap();
        assert!(matches!(resp.status, Status::Timeout(_)));
    }
}
