use crate::types::{Excludes, Response, Status, Uri};
use anyhow::{anyhow, Context, Result};
use check_if_email_exists::{check_email, CheckEmailInput};
use derive_builder::Builder;
use headers::{HeaderMap, HeaderValue};
use hubcaps::{Credentials, Github};
use regex::{Regex, RegexSet};
use reqwest::header;
use std::net::IpAddr;
use std::{collections::HashSet, time::Duration};
use tokio::time::delay_for;
use url::Url;

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
        } else if statuscode.is_success() {
            return Status::Ok(statuscode);
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

    pub fn is_excluded(&self) -> bool {
        matches!(self, Status::Excluded)
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

/// Exclude configuration for the link checker.
#[derive(Debug, Clone)]
pub struct Excludes {
    regex: Option<RegexSet>,
    private_ips: bool,
    link_local_ips: bool,
    loopback_ips: bool,
}

impl Excludes {
    pub(crate) fn from_options(config: &Config) -> Self {
        // exclude_all_private option turns on all "private" excludes,
        // including private IPs, link-local IPs and loopback IPs
        let enable_exclude = |opt| opt || config.exclude_all_private;

        Self {
            regex: RegexSet::new(&config.exclude).ok(),
            private_ips: enable_exclude(config.exclude_private),
            link_local_ips: enable_exclude(config.exclude_link_local),
            loopback_ips: enable_exclude(config.exclude_loopback),
        }
    }
}

pub struct CheckerClient {
    reqwest_client: reqwest::Client,
    github: Option<Github>,
    includes: Option<RegexSet>,
    excludes: Excludes,
    scheme: Option<String>,
    method: reqwest::Method,
    accepted: Option<HashSet<reqwest::StatusCode>>,
}

/// A link checker using an API token for Github links
/// otherwise a normal HTTP client.
#[derive(Builder, Debug)]
#[builder(build_fn(skip))]
#[builder(setter(into))]
pub struct Checker {
    github_token: Option<String>,
    includes: Option<RegexSet>,
    excludes: Excludes,
    #[builder(default = "5")]
    max_redirects: usize,
    user_agent: String,
    allow_insecure: bool,
    scheme: Option<String>,
    custom_headers: HeaderMap,
    method: reqwest::Method,
    accepted: Option<HashSet<http::StatusCode>>,
    timeout: Option<Duration>,
    verbose: bool,
}

pub struct CheckerClient {
    reqwest_client: reqwest::Client,
    github: Option<Github>,
    includes: Option<RegexSet>,
    excludes: Excludes,
    scheme: Option<String>,
    method: reqwest::Method,
    accepted: Option<HashSet<reqwest::StatusCode>>,
    verbose: bool,
}

impl CheckerBuilder {
    /// Creates a new link checker
    // we should consider adding a config struct for this, so that the list
    // of arguments is short
    #[allow(clippy::too_many_arguments)]
    pub fn build(&mut self) -> Result<CheckerClient> {
        // Faking the user agent is necessary for some websites, unfortunately.
        let user_agent = Clone::clone(
            self.user_agent
                .as_ref()
                .ok_or(anyhow!("user_agent must be initialized"))?,
        );

        let mut headers = HeaderMap::new();
        headers.insert(header::USER_AGENT, HeaderValue::from_str(&user_agent)?);
        headers.insert(header::TRANSFER_ENCODING, HeaderValue::from_str("chunked")?);
        // headers.extend(self.custom_headers);

        let allow_insecure = self
            .allow_insecure
            .ok_or(anyhow!("allow_insecure must be initialized"))?;

        let allow_insecure = self.allow_insecure.unwrap_or(false);
        let max_redirects = self.max_redirects.unwrap_or(DEFAULT_MAX_REDIRECTS);

        let builder = reqwest::ClientBuilder::new()
            .gzip(true)
            .default_headers(headers)
            .danger_accept_invalid_certs(allow_insecure)
            .redirect(reqwest::redirect::Policy::limited(max_redirects));

        let builder = match self.timeout {
            Some(t) => builder.timeout(t.ok_or(anyhow!("cannot read timeout"))?),
            None => builder,
        };

        let reqwest_client = builder.build()?;

        let github_token = Clone::clone(
            self.github_token
                .as_ref()
                .ok_or(anyhow!("github_token must be initialized"))?,
        );
        let github = match github_token {
            Some(token) => {
                let github = Github::new(user_agent, Credentials::Token(token))?;
                Some(github)
            }
            None => None,
        };

        let scheme = Clone::clone(
            self.scheme
                .as_ref()
                .ok_or(anyhow!("schememust be initialized"))?,
        );
        let scheme = scheme.map(|s| s.to_lowercase());

        let includes = Clone::clone(
            self.includes
                .as_ref()
                .ok_or(anyhow!("includes must be initialized"))?,
        );

        let excludes = Clone::clone(
            self.excludes
                .as_ref()
                .ok_or(anyhow!("excludes must be initialized"))?,
        );

        let method = Clone::clone(
            self.method
                .as_ref()
                .ok_or(anyhow!("method must be initialized"))?,
        );

        let accepted = Clone::clone(
            self.accepted
                .as_ref()
                .ok_or(anyhow!("accepted must be initialized"))?,
        );
        let verbose = Clone::clone(
            self.verbose
                .as_ref()
                .ok_or(anyhow!("verbosemust be initialized"))?,
        );

        Ok(CheckerClient {
            reqwest_client,
            github,
            includes,
            excludes,
            scheme,
            method,
            accepted,
            verbose,
        })
    }
}

impl CheckerClient {
    async fn check_github(&self, owner: String, repo: String) -> Status {
        match &self.github {
            Some(github) => {
                info!("Check Github: {}/{}", owner, repo);
                let repo = github.repo(owner, repo).get().await;
                match repo {
                    Err(e) => Status::Error(format!("{}", e)),
                    Ok(_) => Status::Ok(http::StatusCode::OK),
                }
            }
            None => Status::Error(
                "GitHub token not specified. To check GitHub links reliably, \
                use `--github-token` flag / `GITHUB_TOKEN` env var."
                    .to_string(),
            ),
        }
    }

    async fn check_normal(&self, url: &Url) -> Status {
        let request = self
            .reqwest_client
            .request(self.method.clone(), url.as_str());
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
        let mut retries: i64 = 3;
        let mut wait: u64 = 1;
        let status = loop {
            let res = self.check_normal(&url).await;
            match res.is_success() {
                true => return res,
                false => {
                    if retries > 0 {
                        retries -= 1;
                        delay_for(Duration::from_secs(wait)).await;
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

    fn in_regex_excludes(&self, input: &str) -> bool {
        if let Some(excludes) = &self.excludes.regex {
            if excludes.is_match(input) {
                return true;
            }
        }
        false
    }

    fn in_ip_excludes(&self, uri: &Uri) -> bool {
        if let Some(ipaddr) = uri.host_ip() {
            if self.excludes.loopback_ips && ipaddr.is_loopback() {
                return true;
            }

            // Note: in a pathological case, an IPv6 address can be IPv4-mapped
            //       (IPv4 address embedded in a IPv6).  We purposefully
            //       don't deal with it here, and assume if an address is IPv6,
            //       we shouldn't attempt to map it to IPv4.
            //       See: https://tools.ietf.org/html/rfc4291#section-2.5.5.2
            if let IpAddr::V4(v4addr) = ipaddr {
                if self.excludes.private_ips && v4addr.is_private() {
                    return true;
                }
                if self.excludes.link_local_ips && v4addr.is_link_local() {
                    return true;
                }
            }
        }

        false
    }

    pub fn excluded(&self, uri: &Uri) -> bool {
        if let Some(includes) = &self.includes {
            if includes.is_match(uri.as_str()) {
                // Includes take precedence over excludes
                return false;
            } else {
                // In case we have includes and no excludes,
                // skip everything that was not included
                if self.excludes.regex.is_none() {
                    return true;
                }
            }
        }
        if self.in_regex_excludes(uri.as_str()) {
            return true;
        }
        if self.in_ip_excludes(&uri) {
            return true;
        }
        if self.scheme.is_none() {
            return false;
        }
        uri.scheme() != self.scheme
    }

    pub async fn check(&self, uri: Uri) -> Response {
        if self.excluded(&uri) {
            return Status::Excluded;
        }

        // if let Some(pb) = self.progress_bar {
        //     pb.set_message(&uri.to_string());
        // }

        let ret = match uri {
            Uri::Website(url) => self.check_real(url).await,
            Uri::Mail(address) => {
                let valid = self.valid_mail(address).await;
                if valid {
                    // TODO: We should not be using a HTTP status code for mail
                    Status::Ok(http::StatusCode::OK)
                } else {
                    Status::Error(format!("Invalid mail address: {}", address))
                }
            }
        };

        ret
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use http::StatusCode;
    use std::time::{Duration, Instant};
    use url::Url;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Note: the standard library as of Rust stable 1.47.0 does not expose
    //       "link-local" or "private" IPv6 checks.  However, one might argue
    //       that these concepts do exist in IPv6, albeit the naming is different.
    //       See: https://en.wikipedia.org/wiki/Link-local_address#IPv6
    //       See: https://en.wikipedia.org/wiki/Private_network#IPv6
    //       See: https://doc.rust-lang.org/stable/std/net/struct.Ipv6Addr.html#method.is_unicast_link_local
    const V4_PRIVATE_CLASS_A: &str = "http://10.0.0.1";
    const V4_PRIVATE_CLASS_B: &str = "http://172.16.0.1";
    const V4_PRIVATE_CLASS_C: &str = "http://192.168.0.1";

    const V4_LOOPBACK: &str = "http://127.0.0.1";
    const V6_LOOPBACK: &str = "http://[::1]";

    const V4_LINK_LOCAL: &str = "http://169.254.0.1";

    // IPv4-Mapped IPv6 addresses (IPv4 embedded in IPv6)
    const V6_MAPPED_V4_PRIVATE_CLASS_A: &str = "http://[::ffff:10.0.0.1]";
    const V6_MAPPED_V4_LINK_LOCAL: &str = "http://[::ffff:169.254.0.1]";

    fn website_url(s: &str) -> Uri {
        Uri::Website(Url::parse(s).expect("Expected valid Website Uri"))
    }

    #[tokio::test]
    async fn test_nonexistent() {
        let res = CheckerBuilder::default()
            .build()
            .unwrap()
            .check(website_url("https://endler.dev/abcd"))
            .await;
        assert!(matches!(res.status, Status::Failed(_)));
    }

    #[tokio::test]
    async fn test_exponential_backoff() {
        let start = Instant::now();
        let uri = Uri::Website(Url::parse("https://endler.dev/abcd").unwrap());
        let res = CheckerBuilder::default().build().unwrap().check(uri).await;
        let end = start.elapsed();

        assert!(matches!(res.status, Status::Failed(_)));

        // on slow connections, this might take a bit longer than nominal backed-off timeout (7 secs)
        assert!(end.as_secs() >= 7);
        assert!(end.as_secs() <= 8);
    }

    #[test]
    fn test_is_github() {
        assert_eq!(
            CheckerBuilder::default()
                .build()
                .unwrap()
                .extract_github("https://github.com/mre/idiomatic-rust")
                .unwrap(),
            ("mre".into(), "idiomatic-rust".into())
        );
    }
    #[tokio::test]
    async fn test_github() {
        assert!(matches!(
            CheckerBuilder::default()
                .build()
                .unwrap()
                .check(website_url("https://github.com/mre/idiomatic-rust"))
                .await
                .status,
            Status::Ok(_)
        ));
    }

    #[tokio::test]
    async fn test_github_nonexistent() {
        let res = CheckerBuilder::default()
            .build()
            .unwrap()
            .check(website_url(
                "https://github.com/mre/idiomatic-rust-doesnt-exist-man",
            ))
            .await
            .status;
        assert!(matches!(res, Status::Error(_)));
    }

    #[tokio::test]
    async fn test_non_github() {
        let res = CheckerBuilder::default()
            .build()
            .unwrap()
            .check(website_url("https://endler.dev"))
            .await
            .status;
        assert!(matches!(res, Status::Ok(_)));
    }

    #[tokio::test]
    async fn test_invalid_ssl() {
        let res = CheckerBuilder::default()
            .build()
            .unwrap()
            .check(website_url("https://expired.badssl.com/"))
            .await;
        assert!(matches!(res.status, Status::Error(_)));

        // Same, but ignore certificate error
        let res = CheckerBuilder::default()
            .allow_insecure(true)
            .build()
            .unwrap()
            .check(website_url("https://expired.badssl.com/"))
            .await;
        assert!(matches!(res.status, Status::Ok(_)));
    }

    #[tokio::test]
    async fn test_custom_headers() {
        let res = CheckerBuilder::default()
            .build()
            .unwrap()
            .check(website_url("https://crates.io/keywords/cassandra"))
            .await;
        assert!(matches!(res.status, Status::Failed(StatusCode::NOT_FOUND)));

        // Try again, but with a custom header.
        // For example, crates.io requires a custom accept header.
        // See https://github.com/rust-lang/crates.io/issues/788
        let mut custom = HeaderMap::new();
        custom.insert(header::ACCEPT, "text/html".parse().unwrap());
        let res = CheckerBuilder::default()
            .custom_headers(custom)
            .build()
            .unwrap()
            .check(website_url("https://crates.io/keywords/cassandra"))
            .await;
        assert!(matches!(res.status, Status::Ok(_)));
    }

    #[tokio::test]
    async fn test_timeout() {
        // Note: this checks response timeout, not connect timeout.
        // To check connect timeout, we'd have to do something more involved,
        // see: https://github.com/LukeMathWalker/wiremock-rs/issues/19
        let mock_delay = Duration::from_millis(20);
        let checker_timeout = Duration::from_millis(10);
        assert!(mock_delay > checker_timeout);

        let mock_server = MockServer::start().await;
        let template = ResponseTemplate::new(200).set_delay(mock_delay);
        Mock::given(method("GET"))
            .respond_with(template)
            .mount(&mock_server)
            .await;

        let checker = CheckerBuilder::default()
            .timeout(checker_timeout)
            .build()
            .unwrap();

        let resp = checker
            .check(Uri::Website(Url::parse(&mock_server.uri()).unwrap()))
            .await;
        assert!(matches!(resp.status, Status::Timeout));
    }

    #[tokio::test]
    async fn test_include_regex() {
        let includes = RegexSet::new(&[r"foo.github.com"]).unwrap();

        let checker = CheckerBuilder::default()
            .includes(includes)
            .build()
            .unwrap();

        assert_eq!(
            checker.excluded(&website_url("https://foo.github.com")),
            false
        );
        assert_eq!(
            checker.excluded(&website_url("https://bar.github.com")),
            true
        );
    }

    #[tokio::test]
    async fn test_exclude_include_regex() {
        let mut excludes = Excludes::default();
        excludes.regex = Some(RegexSet::new(&[r"github.com"]).unwrap());
        let includes = RegexSet::new(&[r"foo.github.com"]).unwrap();

        let checker = CheckerBuilder::default()
            .includes(includes)
            .excludes(excludes)
            .build()
            .unwrap();

        assert_eq!(
            checker.excluded(&website_url("https://foo.github.com")),
            false
        );
        assert_eq!(checker.excluded(&website_url("https://github.com")), true);
        assert_eq!(
            checker.excluded(&website_url("https://bar.github.com")),
            true
        );
    }

    #[tokio::test]
    async fn test_exclude_regex() {
        let mut excludes = Excludes::default();
        excludes.regex =
            Some(RegexSet::new(&[r"github.com", r"[a-z]+\.(org|net)", r"@example.com"]).unwrap());

        let checker = CheckerBuilder::default()
            .excludes(excludes)
            .build()
            .unwrap();

        assert_eq!(checker.excluded(&website_url("http://github.com")), true);
        assert_eq!(checker.excluded(&website_url("http://exclude.org")), true);
        assert_eq!(
            checker.excluded(&Uri::Mail("mail@example.com".to_string())),
            true
        );
        assert_eq!(
            checker.excluded(&Uri::Mail("foo@bar.dev".to_string())),
            false
        );
    }

    #[test]
    fn test_const_sanity() {
        let get_host = |s| {
            Url::parse(s)
                .expect("Expected valid URL")
                .host()
                .expect("Expected host address")
                .to_owned()
        };
        let into_v4 = |host| match host {
            url::Host::Ipv4(ipv4) => ipv4,
            _ => panic!("Not IPv4"),
        };
        let into_v6 = |host| match host {
            url::Host::Ipv6(ipv6) => ipv6,
            _ => panic!("Not IPv6"),
        };

        assert!(into_v4(get_host(V4_PRIVATE_CLASS_A)).is_private());
        assert!(into_v4(get_host(V4_PRIVATE_CLASS_B)).is_private());
        assert!(into_v4(get_host(V4_PRIVATE_CLASS_C)).is_private());

        assert!(into_v4(get_host(V4_LOOPBACK)).is_loopback());
        assert!(into_v6(get_host(V6_LOOPBACK)).is_loopback());

        assert!(into_v4(get_host(V4_LINK_LOCAL)).is_link_local());
    }

    #[test]
    fn test_excludes_no_private_ips_by_default() {
        let checker = CheckerBuilder::default().build().unwrap();

        assert_eq!(checker.excluded(&website_url(V4_PRIVATE_CLASS_A)), false);
        assert_eq!(checker.excluded(&website_url(V4_PRIVATE_CLASS_B)), false);
        assert_eq!(checker.excluded(&website_url(V4_PRIVATE_CLASS_C)), false);
        assert_eq!(checker.excluded(&website_url(V4_LINK_LOCAL)), false);
        assert_eq!(checker.excluded(&website_url(V4_LOOPBACK)), false);

        assert_eq!(checker.excluded(&website_url(V6_LOOPBACK)), false);
    }

    #[test]
    fn test_exclude_private() {
        let mut checker = CheckerBuilder::default().build().unwrap();
        checker.excludes.private_ips = true;

        assert_eq!(checker.excluded(&website_url(V4_PRIVATE_CLASS_A)), true);
        assert_eq!(checker.excluded(&website_url(V4_PRIVATE_CLASS_B)), true);
        assert_eq!(checker.excluded(&website_url(V4_PRIVATE_CLASS_C)), true);
    }

    #[test]
    fn test_exclude_link_local() {
        let mut checker = CheckerBuilder::default().build().unwrap();
        checker.excludes.link_local_ips = true;

        assert_eq!(checker.excluded(&website_url(V4_LINK_LOCAL)), true);
    }

    #[test]
    fn test_exclude_loopback() {
        let mut checker = CheckerBuilder::default().build().unwrap();
        checker.excludes.loopback_ips = true;

        assert_eq!(checker.excluded(&website_url(V4_LOOPBACK)), true);
        assert_eq!(checker.excluded(&website_url(V6_LOOPBACK)), true);
    }

    #[test]
    fn test_exclude_ip_v4_mapped_ip_v6_not_supported() {
        let mut checker = CheckerBuilder::default().build().unwrap();
        checker.excludes.private_ips = true;
        checker.excludes.link_local_ips = true;

        // if these were pure IPv4, we would exclude
        assert_eq!(
            checker.excluded(&website_url(V6_MAPPED_V4_PRIVATE_CLASS_A)),
            false
        );
        assert_eq!(
            checker.excluded(&website_url(V6_MAPPED_V4_LINK_LOCAL)),
            false
        );
    }
}
