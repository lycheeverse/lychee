use anyhow::{anyhow, bail, Context, Result};
use check_if_email_exists::{check_email, CheckEmailInput};
use derive_builder::Builder;
use headers::{HeaderMap, HeaderValue};
use hubcaps::{Credentials, Github};
use regex::{Regex, RegexSet};
use reqwest::header;
use std::{collections::HashSet, time::Duration};
use std::{convert::TryInto, net::IpAddr};
use tokio::time::sleep;
use url::Url;

use crate::types::{Response, Status};
use crate::uri::Uri;
use crate::{excludes::Excludes, Request};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_MAX_REDIRECTS: usize = 5;

#[derive(Debug, Clone)]
pub struct Client {
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

        Ok(Client {
            reqwest_client,
            github,
            includes: self.includes.clone().unwrap_or(None),
            excludes: self.build_excludes(),
            scheme,
            method: self.method.clone().unwrap_or(reqwest::Method::GET),
            accepted: self.accepted.clone().unwrap_or(None),
        })
    }
}

impl Client {
    pub async fn check<T: TryInto<Request>>(&self, request: T) -> Result<Response> {
        let request: Request = match request.try_into() {
            Ok(request) => request,
            Err(_e) => bail!("Invalid URI:"),
        };
        if self.excluded(&request) {
            return Ok(Response::new(request.uri, Status::Excluded, request.source));
        }
        let status = match request.uri {
            Uri::Website(ref url) => self.check_website(&url).await,
            Uri::Mail(ref address) => {
                // TODO: We should not be using a HTTP status code for mail
                match self.valid_mail(&address).await {
                    true => Status::Ok(http::StatusCode::OK),
                    false => Status::Error(format!("Invalid mail address: {}", address)),
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

    pub fn is_mail_excluded(&self) -> bool {
        self.excludes.mail
    }

    pub fn excluded(&self, request: &Request) -> bool {
        if let Some(includes) = &self.includes {
            if includes.is_match(request.uri.as_str()) {
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
        if self.in_regex_excludes(request.uri.as_str()) {
            return true;
        }
        if matches!(request.uri, Uri::Mail(_)) {
            return self.is_mail_excluded();
        }
        if self.in_ip_excludes(&request.uri) {
            return true;
        }
        if self.scheme.is_none() {
            return false;
        }
        request.uri.scheme() != self.scheme
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
    use crate::collector::Input;

    use super::*;
    use http::StatusCode;
    use std::time::{Duration, Instant};
    use url::Url;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Note: the standard library as of Rust stable 1.47.0 does not expose
    // "link-local" or "private" IPv6 checks.  However, one might argue
    // that these concepts do exist in IPv6, albeit the naming is different.
    // See: https://en.wikipedia.org/wiki/Link-local_address#IPv6
    // See: https://en.wikipedia.org/wiki/Private_network#IPv6
    // See: https://doc.rust-lang.org/stable/std/net/struct.Ipv6Addr.html#method.is_unicast_link_local
    const V4_PRIVATE_CLASS_A: &str = "http://10.0.0.1";
    const V4_PRIVATE_CLASS_B: &str = "http://172.16.0.1";
    const V4_PRIVATE_CLASS_C: &str = "http://192.168.0.1";

    const V4_LOOPBACK: &str = "http://127.0.0.1";
    const V6_LOOPBACK: &str = "http://[::1]";

    const V4_LINK_LOCAL: &str = "http://169.254.0.1";

    // IPv4-Mapped IPv6 addresses (IPv4 embedded in IPv6)
    const V6_MAPPED_V4_PRIVATE_CLASS_A: &str = "http://[::ffff:10.0.0.1]";
    const V6_MAPPED_V4_LINK_LOCAL: &str = "http://[::ffff:169.254.0.1]";

    fn website_url(s: &str) -> Request {
        Request::new(
            Uri::Website(Url::parse(s).expect("Expected valid Website URI")),
            Input::Stdin,
        )
    }

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
        assert!(matches!(res.status, Status::Failed(_)));
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
            .check(website_url(&mock_server.uri()))
            .await
            .unwrap();
        let end = start.elapsed();

        assert!(matches!(res.status, Status::Failed(_)));

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
        assert!(matches!(
            ClientBuilder::default()
                .build()
                .unwrap()
                .check(website_url("https://github.com/lycheeverse/lychee"))
                .await
                .unwrap()
                .status,
            Status::Ok(_)
        ));
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
        assert!(matches!(res, Status::Error(_)));
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
            .check(website_url(&mock_server.uri()))
            .await
            .unwrap()
            .status;
        assert!(matches!(res, Status::Ok(_)));
    }

    #[tokio::test]
    async fn test_invalid_ssl() {
        let res = ClientBuilder::default()
            .build()
            .unwrap()
            .check("https://expired.badssl.com/")
            .await
            .unwrap();
        assert!(matches!(res.status, Status::Error(_)));

        // Same, but ignore certificate error
        let res = ClientBuilder::default()
            .allow_insecure(true)
            .build()
            .unwrap()
            .check("https://expired.badssl.com/")
            .await
            .unwrap();
        assert!(matches!(res.status, Status::Ok(_)));
    }

    #[tokio::test]
    async fn test_custom_headers() {
        let res = ClientBuilder::default()
            .build()
            .unwrap()
            .check(website_url("https://crates.io/crates/lychee"))
            .await
            .unwrap();
        assert!(matches!(res.status, Status::Failed(StatusCode::NOT_FOUND)));

        // Try again, but with a custom header.
        // For example, crates.io requires a custom accept header.
        // See https://github.com/rust-lang/crates.io/issues/788
        let mut custom = HeaderMap::new();
        custom.insert(header::ACCEPT, "text/html".parse().unwrap());
        let res = ClientBuilder::default()
            .custom_headers(custom)
            .build()
            .unwrap()
            .check(website_url("https://crates.io/crates/lychee"))
            .await
            .unwrap();
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

        let resp = client.check(website_url(&mock_server.uri())).await.unwrap();
        assert!(matches!(resp.status, Status::Timeout(_)));
    }

    #[tokio::test]
    async fn test_include_regex() {
        let includes = RegexSet::new(&[r"foo.github.com"]).unwrap();

        let client = ClientBuilder::default().includes(includes).build().unwrap();

        assert_eq!(
            client.excluded(&website_url("https://foo.github.com")),
            false
        );
        assert_eq!(
            client.excluded(&website_url("https://bar.github.com")),
            true
        );
    }

    #[tokio::test]
    async fn test_exclude_include_regex() {
        let exclude = Some(RegexSet::new(&[r"github.com"]).unwrap());
        let includes = RegexSet::new(&[r"foo.github.com"]).unwrap();

        let client = ClientBuilder::default()
            .includes(includes)
            .excludes(exclude)
            .build()
            .unwrap();

        assert_eq!(
            client.excluded(&website_url("https://foo.github.com")),
            false
        );
        assert_eq!(client.excluded(&website_url("https://github.com")), true);
        assert_eq!(
            client.excluded(&website_url("https://bar.github.com")),
            true
        );
    }

    #[tokio::test]
    async fn test_exclude_regex() {
        let exclude =
            Some(RegexSet::new(&[r"github.com", r"[a-z]+\.(org|net)", r"@example.com"]).unwrap());

        let client = ClientBuilder::default().excludes(exclude).build().unwrap();

        assert_eq!(client.excluded(&website_url("http://github.com")), true);
        assert_eq!(client.excluded(&website_url("http://exclude.org")), true);
        assert_eq!(
            client.excluded(&Request::new(
                Uri::Mail("mail@example.com".to_string()),
                Input::Stdin,
            )),
            true
        );
        assert_eq!(
            client.excluded(&Request::new(
                Uri::Mail("foo@bar.dev".to_string()),
                Input::Stdin,
            )),
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
        let client = ClientBuilder::default().build().unwrap();

        assert_eq!(client.excluded(&website_url(V4_PRIVATE_CLASS_A)), false);
        assert_eq!(client.excluded(&website_url(V4_PRIVATE_CLASS_B)), false);
        assert_eq!(client.excluded(&website_url(V4_PRIVATE_CLASS_C)), false);
        assert_eq!(client.excluded(&website_url(V4_LINK_LOCAL)), false);
        assert_eq!(client.excluded(&website_url(V4_LOOPBACK)), false);

        assert_eq!(client.excluded(&website_url(V6_LOOPBACK)), false);
    }

    #[test]
    fn test_exclude_private() {
        let mut client = ClientBuilder::default().build().unwrap();
        client.excludes.private_ips = true;

        assert_eq!(client.excluded(&website_url(V4_PRIVATE_CLASS_A)), true);
        assert_eq!(client.excluded(&website_url(V4_PRIVATE_CLASS_B)), true);
        assert_eq!(client.excluded(&website_url(V4_PRIVATE_CLASS_C)), true);
    }

    #[test]
    fn test_exclude_link_local() {
        let mut client = ClientBuilder::default().build().unwrap();
        client.excludes.link_local_ips = true;

        assert_eq!(client.excluded(&website_url(V4_LINK_LOCAL)), true);
    }

    #[test]
    fn test_exclude_loopback() {
        let mut client = ClientBuilder::default().build().unwrap();
        client.excludes.loopback_ips = true;

        assert_eq!(client.excluded(&website_url(V4_LOOPBACK)), true);
        assert_eq!(client.excluded(&website_url(V6_LOOPBACK)), true);
    }

    #[test]
    fn test_exclude_ip_v4_mapped_ip_v6_not_supported() {
        let mut client = ClientBuilder::default().build().unwrap();
        client.excludes.private_ips = true;
        client.excludes.link_local_ips = true;

        // if these were pure IPv4, we would exclude
        assert_eq!(
            client.excluded(&website_url(V6_MAPPED_V4_PRIVATE_CLASS_A)),
            false
        );
        assert_eq!(
            client.excluded(&website_url(V6_MAPPED_V4_LINK_LOCAL)),
            false
        );
    }
}
