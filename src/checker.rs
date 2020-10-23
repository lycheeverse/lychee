use crate::{
    extract::{self, Uri},
    options::Config,
};
use anyhow::anyhow;
use anyhow::{Context, Result};
use check_if_email_exists::{check_email, CheckEmailInput};
use headers::{HeaderMap, HeaderValue};
use hubcaps::{Credentials, Github};
use indicatif::ProgressBar;
use regex::{Regex, RegexSet};
use reqwest::header;
use std::net::IpAddr;
use std::{collections::HashSet, convert::TryFrom, time::Duration};
use tokio::time::delay_for;
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
pub(crate) struct Excludes {
    regex: Option<RegexSet>,
    private_ips: bool,
    link_local_ips: bool,
    loopback_ips: bool,
}

impl Excludes {
    pub fn from_options(config: &Config) -> Self {
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

impl Default for Excludes {
    fn default() -> Self {
        Self {
            regex: None,
            private_ips: false,
            link_local_ips: false,
            loopback_ips: false,
        }
    }
}

/// A link checker using an API token for Github links
/// otherwise a normal HTTP client.
pub(crate) struct Checker<'a> {
    reqwest_client: reqwest::Client,
    github: Github,
    excludes: Excludes,
    scheme: Option<String>,
    method: RequestMethod,
    accepted: Option<HashSet<reqwest::StatusCode>>,
    verbose: bool,
    progress_bar: Option<&'a ProgressBar>,
}

impl<'a> Checker<'a> {
    /// Creates a new link checker
    // we should consider adding a config struct for this, so that the list
    // of arguments is short
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        token: String,
        excludes: Excludes,
        max_redirects: usize,
        user_agent: String,
        allow_insecure: bool,
        scheme: Option<String>,
        custom_headers: HeaderMap,
        method: RequestMethod,
        accepted: Option<HashSet<http::StatusCode>>,
        timeout: Option<Duration>,
        verbose: bool,
        progress_bar: Option<&'a ProgressBar>,
    ) -> Result<Self> {
        let mut headers = HeaderMap::new();
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

        let builder = match timeout {
            Some(timeout) => builder.timeout(timeout),
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
            progress_bar,
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

    fn status_message(&self, status: &Status, uri: &Uri) -> Option<String> {
        match status {
            Status::Ok(code) => {
                if self.verbose {
                    Some(format!("✅{} [{}]", uri, code))
                } else {
                    None
                }
            }
            Status::Failed(code) => Some(format!("🚫{} [{}]", uri, code)),
            Status::Redirected => {
                if self.verbose {
                    Some(format!("🔀️{}", uri))
                } else {
                    None
                }
            }
            Status::Excluded => {
                if self.verbose {
                    Some(format!("👻{}", uri))
                } else {
                    None
                }
            }
            Status::Error(e) => Some(format!("⚡ {} ({})", uri, e)),
            Status::Timeout => Some(format!("⌛{}", uri)),
        }
    }

    pub async fn check(&self, uri: &extract::Uri) -> Status {
        if self.excluded(&uri) {
            return Status::Excluded;
        }

        if let Some(pb) = self.progress_bar {
            pb.set_message(&uri.to_string());
        }

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

        if let Some(pb) = self.progress_bar {
            pb.inc(1);
            // regular println! inteferes with progress bar
            if let Some(message) = self.status_message(&ret, uri) {
                pb.println(message);
            }
        } else if let Some(message) = self.status_message(&ret, uri) {
            println!("{}", message);
        }

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

    fn get_checker(allow_insecure: bool, custom_headers: HeaderMap) -> Checker<'static> {
        let checker = Checker::try_new(
            "DUMMY_GITHUB_TOKEN".to_string(),
            Excludes::default(),
            5,
            "curl/7.71.1".to_string(),
            allow_insecure,
            None,
            custom_headers,
            RequestMethod::GET,
            None,
            None,
            false,
            None,
        )
        .unwrap();
        checker
    }

    fn website_url(s: &str) -> Uri {
        Uri::Website(Url::parse(s).expect("Expected valid Website Uri"))
    }

    #[tokio::test]
    async fn test_nonexistent() {
        let res = get_checker(false, HeaderMap::new())
            .check(&website_url("https://endler.dev/abcd"))
            .await;
        assert!(matches!(res, Status::Failed(_)));
    }

    #[tokio::test]
    async fn test_exponetial_backoff() {
        let start = Instant::now();
        let res = get_checker(false, HeaderMap::new())
            .check(&Uri::Website(
                Url::parse("https://endler.dev/abcd").unwrap(),
            ))
            .await;
        let end = start.elapsed();

        assert!(matches!(res, Status::Failed(_)));
        assert!(matches!(end.as_secs(), 7));
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
                .check(&website_url("https://github.com/mre/idiomatic-rust"))
                .await,
            Status::Ok(_)
        ));
    }

    #[tokio::test]
    async fn test_github_nonexistent() {
        let res = get_checker(false, HeaderMap::new())
            .check(&website_url(
                "https://github.com/mre/idiomatic-rust-doesnt-exist-man",
            ))
            .await;
        assert!(matches!(res, Status::Error(_)));
    }

    #[tokio::test]
    async fn test_non_github() {
        let res = get_checker(false, HeaderMap::new())
            .check(&website_url("https://endler.dev"))
            .await;
        assert!(matches!(res, Status::Ok(_)));
    }

    #[tokio::test]
    async fn test_invalid_ssl() {
        let res = get_checker(false, HeaderMap::new())
            .check(&website_url("https://expired.badssl.com/"))
            .await;
        assert!(matches!(res, Status::Error(_)));

        // Same, but ignore certificate error
        let res = get_checker(true, HeaderMap::new())
            .check(&website_url("https://expired.badssl.com/"))
            .await;
        assert!(matches!(res, Status::Ok(_)));
    }

    #[tokio::test]
    async fn test_custom_headers() {
        let res = get_checker(false, HeaderMap::new())
            .check(&website_url("https://crates.io/keywords/cassandra"))
            .await;
        assert!(matches!(res, Status::Failed(StatusCode::NOT_FOUND)));

        // Try again, but with a custom header.
        // For example, crates.io requires a custom accept header.
        // See https://github.com/rust-lang/crates.io/issues/788
        let mut custom = HeaderMap::new();
        custom.insert(header::ACCEPT, "text/html".parse().unwrap());
        let res = get_checker(true, custom)
            .check(&website_url("https://crates.io/keywords/cassandra"))
            .await;
        assert!(matches!(res, Status::Ok(_)));
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

        let checker = Checker::try_new(
            "DUMMY_GITHUB_TOKEN".to_string(),
            Excludes::default(),
            5,
            "curl/7.71.1".to_string(),
            true,
            None,
            HeaderMap::new(),
            RequestMethod::GET,
            None,
            Some(checker_timeout),
            false,
            None,
        )
        .expect("Expected successful instantiation");

        let resp = checker
            .check(&Uri::Website(Url::parse(&mock_server.uri()).unwrap()))
            .await;
        assert!(matches!(resp, Status::Timeout));
    }

    #[tokio::test]
    async fn test_exclude_regex() {
        let mut excludes = Excludes::default();
        excludes.regex =
            Some(RegexSet::new(&[r"github.com", r"[a-z]+\.(org|net)", r"@example.com"]).unwrap());

        let checker = Checker::try_new(
            "DUMMY_GITHUB_TOKEN".to_string(),
            excludes,
            5,
            "curl/7.71.1".to_string(),
            true,
            None,
            HeaderMap::new(),
            RequestMethod::GET,
            None,
            None,
            false,
            None,
        )
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
        let checker = get_checker(false, HeaderMap::new());

        assert_eq!(checker.excluded(&website_url(V4_PRIVATE_CLASS_A)), false);
        assert_eq!(checker.excluded(&website_url(V4_PRIVATE_CLASS_B)), false);
        assert_eq!(checker.excluded(&website_url(V4_PRIVATE_CLASS_C)), false);
        assert_eq!(checker.excluded(&website_url(V4_LINK_LOCAL)), false);
        assert_eq!(checker.excluded(&website_url(V4_LOOPBACK)), false);

        assert_eq!(checker.excluded(&website_url(V6_LOOPBACK)), false);
    }

    #[test]
    fn test_exclude_private() {
        let mut checker = get_checker(false, HeaderMap::new());
        checker.excludes.private_ips = true;

        assert_eq!(checker.excluded(&website_url(V4_PRIVATE_CLASS_A)), true);
        assert_eq!(checker.excluded(&website_url(V4_PRIVATE_CLASS_B)), true);
        assert_eq!(checker.excluded(&website_url(V4_PRIVATE_CLASS_C)), true);
    }

    #[test]
    fn test_exclude_link_local() {
        let mut checker = get_checker(false, HeaderMap::new());
        checker.excludes.link_local_ips = true;

        assert_eq!(checker.excluded(&website_url(V4_LINK_LOCAL)), true);
    }

    #[test]
    fn test_exclude_loopback() {
        let mut checker = get_checker(false, HeaderMap::new());
        checker.excludes.loopback_ips = true;

        assert_eq!(checker.excluded(&website_url(V4_LOOPBACK)), true);
        assert_eq!(checker.excluded(&website_url(V6_LOOPBACK)), true);
    }

    #[test]
    fn test_exclude_ip_v4_mapped_ip_v6_not_supported() {
        let mut checker = get_checker(false, HeaderMap::new());
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
