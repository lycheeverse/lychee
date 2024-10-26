//! Handler of link checking operations.
//!
//! This module defines two structs, [`Client`] and [`ClientBuilder`].
//! `Client` handles incoming requests and returns responses.
//! `ClientBuilder` exposes a finer level of granularity for building
//! a `Client`.
//!
//! For convenience, a free function [`check`] is provided for ad-hoc
//! link checks.
#![allow(
    clippy::module_name_repetitions,
    clippy::struct_excessive_bools,
    clippy::default_trait_access,
    clippy::used_underscore_binding
)]
use std::{collections::HashSet, path::Path, sync::Arc, time::Duration};

use http::{
    header::{HeaderMap, HeaderValue},
    StatusCode,
};
use log::{debug, warn};
use octocrab::Octocrab;
use regex::RegexSet;
use reqwest::{header, redirect};
use reqwest_cookie_store::CookieStoreMutex;
use secrecy::{ExposeSecret, SecretString};
use typed_builder::TypedBuilder;

use crate::{
    chain::RequestChain,
    checker::file::FileChecker,
    checker::{mail::MailChecker, website::WebsiteChecker},
    filter::{Excludes, Filter, Includes},
    remap::Remaps,
    utils::fragment_checker::FragmentChecker,
    Base, BasicAuthCredentials, ErrorKind, Request, Response, Result, Status, Uri,
};

/// Default number of redirects before a request is deemed as failed, 5.
pub const DEFAULT_MAX_REDIRECTS: usize = 5;
/// Default number of retries before a request is deemed as failed, 3.
pub const DEFAULT_MAX_RETRIES: u64 = 3;
/// Default wait time in seconds between retries, 1.
pub const DEFAULT_RETRY_WAIT_TIME_SECS: usize = 1;
/// Default timeout in seconds before a request is deemed as failed, 20.
pub const DEFAULT_TIMEOUT_SECS: usize = 20;
/// Default user agent, `lychee-<PKG_VERSION>`.
pub const DEFAULT_USER_AGENT: &str = concat!("lychee/", env!("CARGO_PKG_VERSION"));

// Constants currently not configurable by the user.
/// A timeout for only the connect phase of a [`Client`].
const CONNECT_TIMEOUT: u64 = 10;
/// TCP keepalive.
///
/// See <https://tldp.org/HOWTO/TCP-Keepalive-HOWTO/overview.html> for more
/// information.
const TCP_KEEPALIVE: u64 = 60;

/// Builder for [`Client`].
///
/// See crate-level documentation for usage example.
#[derive(TypedBuilder, Debug, Clone)]
#[builder(field_defaults(default, setter(into)))]
pub struct ClientBuilder {
    /// Optional GitHub token used for GitHub links.
    ///
    /// This allows much more request before getting rate-limited.
    ///
    /// # Rate-limiting Defaults
    ///
    /// As of Feb 2022, it's 60 per hour without GitHub token v.s.
    /// 5000 per hour with token.
    github_token: Option<SecretString>,

    /// Remap URIs matching a pattern to a different URI.
    ///
    /// This makes it possible to remap any HTTP/HTTPS endpoint to a different
    /// HTTP/HTTPS one. This feature could also be used to proxy
    /// certain requests.
    ///
    /// # Usage Notes
    ///
    /// Use with caution because a large set of remapping rules may cause
    /// performance issues.
    ///
    /// Furthermore rules are executed sequentially and multiple mappings for
    /// the same URI are allowed, so it is up to the library user's discretion to
    /// make sure rules don't conflict with each other.
    remaps: Option<Remaps>,

    /// Automatically append file extensions to `file://` URIs as needed
    fallback_extensions: Vec<String>,

    /// Links matching this set of regular expressions are **always** checked.
    ///
    /// This has higher precedence over [`ClientBuilder::excludes`], **but**
    /// has lower precedence compared to any other `exclude_` fields or
    /// [`ClientBuilder::schemes`] below.
    includes: Option<RegexSet>,

    /// Links matching this set of regular expressions are ignored, **except**
    /// when a link also matches against [`ClientBuilder::includes`].
    excludes: Option<RegexSet>,

    /// When `true`, exclude all private network addresses.
    ///
    /// This effectively turns on the following fields:
    /// - [`ClientBuilder::exclude_private_ips`]
    /// - [`ClientBuilder::exclude_link_local_ips`]
    /// - [`ClientBuilder::exclude_loopback_ips`]
    exclude_all_private: bool,

    /// When `true`, exclude private IP addresses.
    ///
    /// # IPv4
    ///
    /// The private address ranges are defined in [IETF RFC 1918] and include:
    ///
    ///  - `10.0.0.0/8`
    ///  - `172.16.0.0/12`
    ///  - `192.168.0.0/16`
    ///
    /// # IPv6
    ///
    /// The address is a unique local address (`fc00::/7`).
    ///
    /// This property is defined in [IETF RFC 4193].
    ///
    /// # Note
    ///
    /// Unicast site-local network was defined in [IETF RFC 4291], but was fully
    /// deprecated in [IETF RFC 3879]. So it is **NOT** considered as private on
    /// this purpose.
    ///
    /// [IETF RFC 1918]: https://tools.ietf.org/html/rfc1918
    /// [IETF RFC 4193]: https://tools.ietf.org/html/rfc4193
    /// [IETF RFC 4291]: https://tools.ietf.org/html/rfc4291
    /// [IETF RFC 3879]: https://tools.ietf.org/html/rfc3879
    exclude_private_ips: bool,

    /// When `true`, exclude link-local IPs.
    ///
    /// # IPv4
    ///
    /// The address is `169.254.0.0/16`.
    ///
    /// This property is defined by [IETF RFC 3927].
    ///
    /// # IPv6
    ///
    /// The address is a unicast address with link-local scope,  as defined in
    /// [RFC 4291].
    ///
    /// A unicast address has link-local scope if it has the prefix `fe80::/10`,
    /// as per [RFC 4291 section 2.4].
    ///
    /// [IETF RFC 3927]: https://tools.ietf.org/html/rfc3927
    /// [RFC 4291]: https://tools.ietf.org/html/rfc4291
    /// [RFC 4291 section 2.4]: https://tools.ietf.org/html/rfc4291#section-2.4
    exclude_link_local_ips: bool,

    /// When `true`, exclude loopback IP addresses.
    ///
    /// # IPv4
    ///
    /// This is a loopback address (`127.0.0.0/8`).
    ///
    /// This property is defined by [IETF RFC 1122].
    ///
    /// # IPv6
    ///
    /// This is the loopback address (`::1`), as defined in
    /// [IETF RFC 4291 section 2.5.3].
    ///
    /// [IETF RFC 1122]: https://tools.ietf.org/html/rfc1122
    /// [IETF RFC 4291 section 2.5.3]: https://tools.ietf.org/html/rfc4291#section-2.5.3
    exclude_loopback_ips: bool,

    /// When `true`, check mail addresses.
    include_mail: bool,

    /// Maximum number of redirects per request before returning an error.
    ///
    /// Defaults to [`DEFAULT_MAX_REDIRECTS`].
    #[builder(default = DEFAULT_MAX_REDIRECTS)]
    max_redirects: usize,

    /// Maximum number of retries per request before returning an error.
    ///
    /// Defaults to [`DEFAULT_MAX_RETRIES`].
    #[builder(default = DEFAULT_MAX_RETRIES)]
    max_retries: u64,

    /// User-agent used for checking links.
    ///
    /// Defaults to [`DEFAULT_USER_AGENT`].
    ///
    /// # Notes
    ///
    /// This may be helpful for bypassing certain firewalls.
    // Faking the user agent is necessary for some websites, unfortunately.
    // Otherwise we get a 403 from the firewall (e.g. Sucuri/Cloudproxy on ldra.com).
    #[builder(default_code = "String::from(DEFAULT_USER_AGENT)")]
    user_agent: String,

    /// When `true`, accept invalid SSL certificates.
    ///
    /// # Warning
    ///
    /// You should think very carefully before allowing invalid SSL
    /// certificates. It will accept any certificate for any site to be trusted
    /// including expired certificates. This introduces significant
    /// vulnerabilities, and should only be used as a last resort.
    // TODO: We should add a warning message in CLI. (Lucius, Jan 2023)
    allow_insecure: bool,

    /// Set of accepted URL schemes.
    ///
    /// Only links with matched URI schemes are checked. This has no effect when
    /// it's empty.
    schemes: HashSet<String>,

    /// Default [headers] for every request.
    ///
    /// This allows working around validation issues on some websites. See also
    /// [here] for usage examples.
    ///
    /// [headers]: https://docs.rs/http/latest/http/header/struct.HeaderName.html
    /// [here]: https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html#method.default_headers
    custom_headers: HeaderMap,

    /// HTTP method used for requests, e.g. `GET` or `HEAD`.
    #[builder(default = reqwest::Method::GET)]
    method: reqwest::Method,

    /// Set of accepted return codes / status codes.
    ///
    /// Unmatched return codes/ status codes are deemed as errors.
    accepted: Option<HashSet<StatusCode>>,

    /// Response timeout per request in seconds.
    timeout: Option<Duration>,

    /// Base for resolving paths.
    ///
    /// E.g. if the base is `/home/user/` and the path is `file.txt`, the
    /// resolved path would be `/home/user/file.txt`.
    base: Option<Base>,

    /// Initial time between retries of failed requests.
    ///
    /// Defaults to [`DEFAULT_RETRY_WAIT_TIME_SECS`].
    ///
    /// # Notes
    ///
    /// For each request, the wait time increases using an exponential backoff
    /// mechanism. For example, if the value is 1 second, then it waits for
    /// 2 ^ (N-1) seconds before the N-th retry.
    ///
    /// This prevents spending too much system resources on slow responders and
    /// prioritizes other requests.
    #[builder(default_code = "Duration::from_secs(DEFAULT_RETRY_WAIT_TIME_SECS as u64)")]
    retry_wait_time: Duration,

    /// When `true`, requires using HTTPS when it's available.
    ///
    /// This would treat unencrypted links as errors when HTTPS is available.
    /// It has no effect on non-HTTP schemes or if the URL doesn't support
    /// HTTPS.
    require_https: bool,

    /// Cookie store used for requests.
    ///
    /// See <https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html#method.cookie_store>
    cookie_jar: Option<Arc<CookieStoreMutex>>,

    /// Enable the checking of fragments in links.
    include_fragments: bool,

    /// Requests run through this chain where each item in the chain
    /// can modify the request. A chained item can also decide to exit
    /// early and return a status, so that subsequent chain items are
    /// skipped and the lychee-internal request chain is not activated.
    plugin_request_chain: RequestChain,
}

impl Default for ClientBuilder {
    #[must_use]
    #[inline]
    fn default() -> Self {
        Self::builder().build()
    }
}

impl ClientBuilder {
    /// Instantiates a [`Client`].
    ///
    /// # Errors
    ///
    /// Returns an `Err` if:
    /// - The user-agent contains characters other than ASCII 32-127.
    /// - The reqwest client cannot be instantiated. This occurs if a TLS
    ///   backend cannot be initialized or the resolver fails to load the system
    ///   configuration. See [here].
    /// - The GitHub client cannot be created. Since the implementation also
    ///   uses reqwest under the hood, this errors in the same circumstances as
    ///   the last one.
    ///
    /// [here]: https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html#errors
    pub fn client(self) -> Result<Client> {
        let Self {
            user_agent,
            custom_headers: mut headers,
            ..
        } = self;

        if let Some(prev_user_agent) =
            headers.insert(header::USER_AGENT, HeaderValue::try_from(&user_agent)?)
        {
            debug!(
                "Found user-agent in headers: {}. Overriding it with {user_agent}.",
                prev_user_agent.to_str().unwrap_or("�"),
            );
        };

        headers.insert(
            header::TRANSFER_ENCODING,
            HeaderValue::from_static("chunked"),
        );

        // Custom redirect policy to enable logging of redirects.
        let max_redirects = self.max_redirects;
        let redirect_policy = redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() > max_redirects {
                attempt.error("too many redirects")
            } else {
                debug!("Redirecting to {}", attempt.url());
                attempt.follow()
            }
        });

        let mut builder = reqwest::ClientBuilder::new()
            .gzip(true)
            .default_headers(headers)
            .danger_accept_invalid_certs(self.allow_insecure)
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT))
            .tcp_keepalive(Duration::from_secs(TCP_KEEPALIVE))
            .redirect(redirect_policy);

        if let Some(cookie_jar) = self.cookie_jar {
            builder = builder.cookie_provider(cookie_jar);
        }

        let reqwest_client = match self.timeout {
            Some(t) => builder.timeout(t),
            None => builder,
        }
        .build()
        .map_err(ErrorKind::NetworkRequest)?;

        let github_client = match self.github_token.as_ref().map(ExposeSecret::expose_secret) {
            Some(token) if !token.is_empty() => Some(
                Octocrab::builder()
                    .personal_token(token.to_string())
                    .build()
                    // this is essentially the same reqwest::ClientBuilder::build error
                    // see https://docs.rs/octocrab/0.18.1/src/octocrab/lib.rs.html#360-364
                    .map_err(ErrorKind::BuildGithubClient)?,
            ),
            _ => None,
        };

        let filter = Filter {
            includes: self.includes.map(|regex| Includes { regex }),
            excludes: self.excludes.map(|regex| Excludes { regex }),
            schemes: self.schemes,
            // exclude_all_private option turns on all "private" excludes,
            // including private IPs, link-local IPs and loopback IPs
            exclude_private_ips: self.exclude_all_private || self.exclude_private_ips,
            exclude_link_local_ips: self.exclude_all_private || self.exclude_link_local_ips,
            exclude_loopback_ips: self.exclude_all_private || self.exclude_loopback_ips,
            include_mail: self.include_mail,
        };

        let website_checker = WebsiteChecker::new(
            self.method,
            self.retry_wait_time,
            self.max_retries,
            reqwest_client,
            self.accepted,
            github_client,
            self.require_https,
            self.plugin_request_chain,
        );

        Ok(Client {
            remaps: self.remaps,
            filter,
            email_checker: MailChecker::new(),
            website_checker,
            file_checker: FileChecker::new(
                self.base,
                self.fallback_extensions,
                self.include_fragments,
            ),
            fragment_checker: FragmentChecker::new(),
        })
    }
}

/// Handles incoming requests and returns responses.
///
/// See [`ClientBuilder`] which contains sane defaults for all configuration
/// options.
#[derive(Debug, Clone)]
pub struct Client {
    /// Optional remapping rules for URIs matching pattern.
    remaps: Option<Remaps>,

    /// Rules to decided whether each link should be checked or ignored.
    filter: Filter,

    /// A checker for website URLs.
    website_checker: WebsiteChecker,

    /// A checker for file URLs.
    file_checker: FileChecker,

    /// A checker for email URLs.
    email_checker: MailChecker,

    /// Caches Fragments
    fragment_checker: FragmentChecker,
}

impl Client {
    /// Check a single request.
    ///
    /// `request` can be either a [`Request`] or a type that can be converted
    /// into it. In any case, it must represent a valid URI.
    ///
    /// # Errors
    ///
    /// Returns an `Err` if:
    /// - `request` does not represent a valid URI.
    /// - Encrypted connection for a HTTP URL is available but unused. (Only
    ///   checked when `Client::require_https` is `true`.)
    #[allow(clippy::missing_panics_doc)]
    pub async fn check<T, E>(&self, request: T) -> Result<Response>
    where
        Request: TryFrom<T, Error = E>,
        ErrorKind: From<E>,
    {
        let Request {
            ref mut uri,
            credentials,
            source,
            ..
        } = request.try_into()?;

        // Allow filtering based on element and attribute
        // if !self.filter.is_allowed(uri) {
        //     return Ok(Response::new(
        //         uri.clone(),
        //         Status::Excluded,
        //         source,
        //     ));
        // }

        self.remap(uri)?;

        if self.is_excluded(uri) {
            return Ok(Response::new(uri.clone(), Status::Excluded, source));
        }

        let status = match uri.scheme() {
            // We don't check tel: URIs
            _ if uri.is_tel() => Status::Excluded,
            _ if uri.is_file() => self.check_file(uri).await,
            _ if uri.is_mail() => self.check_mail(uri).await,
            _ => self.check_website(uri, credentials).await?,
        };

        Ok(Response::new(uri.clone(), status, source))
    }

    /// Check a single file using the file checker.
    pub async fn check_file(&self, uri: &Uri) -> Status {
        self.file_checker.check(uri).await
    }

    /// Remap `uri` using the client-defined remapping rules.
    ///
    /// # Errors
    ///
    /// Returns an `Err` if the final, remapped `uri` is not a valid URI.
    pub fn remap(&self, uri: &mut Uri) -> Result<()> {
        if let Some(ref remaps) = self.remaps {
            uri.url = remaps.remap(&uri.url)?;
        }
        Ok(())
    }

    /// Returns whether the given `uri` should be ignored from checking.
    #[must_use]
    pub fn is_excluded(&self, uri: &Uri) -> bool {
        self.filter.is_excluded(uri)
    }

    /// Checks the given URI of a website.
    ///
    /// # Errors
    ///
    /// This returns an `Err` if
    /// - The URI is invalid.
    /// - The request failed.
    /// - The response status code is not accepted.
    /// - The URI cannot be converted to HTTPS.
    pub async fn check_website(
        &self,
        uri: &Uri,
        credentials: Option<BasicAuthCredentials>,
    ) -> Result<Status> {
        self.website_checker.check_website(uri, credentials).await
    }

    /// Checks a `mailto` URI.
    pub async fn check_mail(&self, uri: &Uri) -> Status {
        self.email_checker.check_mail(uri).await
    }

    /// Checks a `file` URI's fragment.
    pub async fn check_fragment(&self, path: &Path, uri: &Uri) -> Status {
        match self.fragment_checker.check(path, &uri.url).await {
            Ok(true) => Status::Ok(StatusCode::OK),
            Ok(false) => ErrorKind::InvalidFragment(uri.clone()).into(),
            Err(err) => {
                warn!("Skipping fragment check due to the following error: {err}");
                Status::Ok(StatusCode::OK)
            }
        }
    }
}

/// A shorthand function to check a single URI.
///
/// This provides the simplest link check utility without having to create a
/// [`Client`]. For more complex scenarios, see documentation of
/// [`ClientBuilder`] instead.
///
/// # Errors
///
/// Returns an `Err` if:
/// - The request client cannot be built (see [`ClientBuilder::client`] for
///   failure cases).
/// - The request cannot be checked (see [`Client::check`] for failure cases).
pub async fn check<T, E>(request: T) -> Result<Response>
where
    Request: TryFrom<T, Error = E>,
    ErrorKind: From<E>,
{
    let client = ClientBuilder::builder().build().client()?;
    client.check(request).await
}

#[cfg(test)]
mod tests {
    use std::{
        fs::File,
        time::{Duration, Instant},
    };

    use async_trait::async_trait;
    use http::{header::HeaderMap, StatusCode};
    use reqwest::header;
    use tempfile::tempdir;
    use wiremock::matchers::path;

    use super::ClientBuilder;
    use crate::{
        chain::{ChainResult, Handler, RequestChain},
        mock_server,
        test_utils::get_mock_client_response,
        ErrorKind, Request, Status, Uri,
    };

    #[tokio::test]
    async fn test_nonexistent() {
        let mock_server = mock_server!(StatusCode::NOT_FOUND);
        let res = get_mock_client_response(mock_server.uri()).await;

        assert!(res.status().is_error());
    }

    #[tokio::test]
    async fn test_nonexistent_with_path() {
        let res = get_mock_client_response("http://127.0.0.1/invalid").await;
        assert!(res.status().is_error());
    }

    #[tokio::test]
    async fn test_github() {
        let res = get_mock_client_response("https://github.com/lycheeverse/lychee").await;
        assert!(res.status().is_success());
    }

    #[tokio::test]
    async fn test_github_nonexistent_repo() {
        let res = get_mock_client_response("https://github.com/lycheeverse/not-lychee").await;
        assert!(res.status().is_error());
    }

    #[tokio::test]
    async fn test_github_nonexistent_file() {
        let res = get_mock_client_response(
            "https://github.com/lycheeverse/lychee/blob/master/NON_EXISTENT_FILE.md",
        )
        .await;
        assert!(res.status().is_error());
    }

    #[tokio::test]
    async fn test_youtube() {
        // This is applying a quirk. See the quirks module.
        let res = get_mock_client_response("https://www.youtube.com/watch?v=NlKuICiT470&list=PLbWDhxwM_45mPVToqaIZNbZeIzFchsKKQ&index=7").await;
        assert!(res.status().is_success());

        let res = get_mock_client_response("https://www.youtube.com/watch?v=invalidNlKuICiT470&list=PLbWDhxwM_45mPVToqaIZNbZeIzFchsKKQ&index=7").await;
        assert!(res.status().is_error());
    }

    #[tokio::test]
    async fn test_basic_auth() {
        let mut r: Request = "https://authenticationtest.com/HTTPAuth/"
            .try_into()
            .unwrap();

        let res = get_mock_client_response(r.clone()).await;
        assert_eq!(res.status().code(), Some(401.try_into().unwrap()));

        r.credentials = Some(crate::BasicAuthCredentials {
            username: "user".into(),
            password: "pass".into(),
        });

        let res = get_mock_client_response(r).await;
        assert!(res.status().is_success());
    }

    #[tokio::test]
    async fn test_non_github() {
        let mock_server = mock_server!(StatusCode::OK);
        let res = get_mock_client_response(mock_server.uri()).await;

        assert!(res.status().is_success());
    }

    #[tokio::test]
    async fn test_invalid_ssl() {
        let res = get_mock_client_response("https://expired.badssl.com/").await;

        assert!(res.status().is_error());

        // Same, but ignore certificate error
        let res = ClientBuilder::builder()
            .allow_insecure(true)
            .build()
            .client()
            .unwrap()
            .check("https://expired.badssl.com/")
            .await
            .unwrap();
        assert!(res.status().is_success());
    }

    #[tokio::test]
    async fn test_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("temp");
        File::create(file).unwrap();
        let uri = format!("file://{}", dir.path().join("temp").to_str().unwrap());

        let res = get_mock_client_response(uri).await;
        assert!(res.status().is_success());
    }

    #[tokio::test]
    async fn test_custom_headers() {
        // See https://github.com/rust-lang/crates.io/issues/788
        let mut custom = HeaderMap::new();
        custom.insert(header::ACCEPT, "text/html".parse().unwrap());
        let res = ClientBuilder::builder()
            .custom_headers(custom)
            .build()
            .client()
            .unwrap()
            .check("https://crates.io/crates/lychee")
            .await
            .unwrap();
        assert!(res.status().is_success());
    }

    #[tokio::test]
    async fn test_exclude_mail_by_default() {
        let client = ClientBuilder::builder()
            .exclude_all_private(true)
            .build()
            .client()
            .unwrap();
        assert!(client.is_excluded(&Uri {
            url: "mailto://mail@example.com".try_into().unwrap()
        }));
    }

    #[tokio::test]
    async fn test_include_mail() {
        let client = ClientBuilder::builder()
            .include_mail(false)
            .exclude_all_private(true)
            .build()
            .client()
            .unwrap();
        assert!(client.is_excluded(&Uri {
            url: "mailto://mail@example.com".try_into().unwrap()
        }));

        let client = ClientBuilder::builder()
            .include_mail(true)
            .exclude_all_private(true)
            .build()
            .client()
            .unwrap();
        assert!(!client.is_excluded(&Uri {
            url: "mailto://mail@example.com".try_into().unwrap()
        }));
    }

    #[tokio::test]
    async fn test_include_tel() {
        let client = ClientBuilder::builder().build().client().unwrap();
        assert!(client.is_excluded(&Uri {
            url: "tel:1234567890".try_into().unwrap()
        }));
    }

    #[tokio::test]
    async fn test_require_https() {
        let client = ClientBuilder::builder().build().client().unwrap();
        let res = client.check("http://example.com").await.unwrap();
        assert!(res.status().is_success());

        // Same request will fail if HTTPS is required
        let client = ClientBuilder::builder()
            .require_https(true)
            .build()
            .client()
            .unwrap();
        let res = client.check("http://example.com").await.unwrap();
        assert!(res.status().is_error());
    }

    #[tokio::test]
    async fn test_timeout() {
        // Note: this checks response timeout, not connect timeout.
        // To check connect timeout, we'd have to do something more involved,
        // see: https://github.com/LukeMathWalker/wiremock-rs/issues/19
        let mock_delay = Duration::from_millis(20);
        let checker_timeout = Duration::from_millis(10);
        assert!(mock_delay > checker_timeout);

        let mock_server = mock_server!(StatusCode::OK, set_delay(mock_delay));

        let client = ClientBuilder::builder()
            .timeout(checker_timeout)
            .build()
            .client()
            .unwrap();

        let res = client.check(mock_server.uri()).await.unwrap();
        assert!(res.status().is_timeout());
    }

    #[tokio::test]
    async fn test_exponential_backoff() {
        let mock_delay = Duration::from_millis(20);
        let checker_timeout = Duration::from_millis(10);
        assert!(mock_delay > checker_timeout);

        let mock_server = mock_server!(StatusCode::OK, set_delay(mock_delay));

        // Perform a warm-up request to ensure the lazy regexes
        // in lychee-lib/src/quirks/mod.rs are compiled.
        // On some platforms, this can take some time(approx. 110ms),
        // which should not be counted in the test.
        let warm_up_client = ClientBuilder::builder()
            .max_retries(0_u64)
            .build()
            .client()
            .unwrap();
        let _res = warm_up_client.check(mock_server.uri()).await.unwrap();

        let client = ClientBuilder::builder()
            .timeout(checker_timeout)
            .max_retries(3_u64)
            .retry_wait_time(Duration::from_millis(50))
            .build()
            .client()
            .unwrap();

        // Summary:
        // 1. First request fails with timeout (after 10ms)
        // 2. Retry after 50ms (total 60ms)
        // 3. Second request fails with timeout (after 10ms)
        // 4. Retry after 100ms (total 160ms)
        // 5. Third request fails with timeout (after 10ms)
        // 6. Retry after 200ms (total 360ms)
        // Total: 360ms

        let start = Instant::now();
        let res = client.check(mock_server.uri()).await.unwrap();
        let end = start.elapsed();

        assert!(res.status().is_error());

        // on slow connections, this might take a bit longer than nominal
        // backed-off timeout (7 secs)
        assert!((350..=550).contains(&end.as_millis()));
    }

    #[tokio::test]
    async fn test_avoid_reqwest_panic() {
        let client = ClientBuilder::builder().build().client().unwrap();
        // This request will result in an Unsupported status, but it won't panic
        let res = client.check("http://\"").await.unwrap();

        assert!(matches!(
            res.status(),
            Status::Unsupported(ErrorKind::BuildRequestClient(_))
        ));
        assert!(res.status().is_unsupported());
    }

    #[tokio::test]
    async fn test_max_redirects() {
        let mock_server = wiremock::MockServer::start().await;

        let ok_uri = format!("{}/ok", &mock_server.uri());
        let redirect_uri = format!("{}/redirect", &mock_server.uri());

        // Set up permanent redirect loop
        let redirect = wiremock::ResponseTemplate::new(StatusCode::PERMANENT_REDIRECT)
            .insert_header("Location", ok_uri.as_str());
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(path("/redirect"))
            .respond_with(redirect)
            .mount(&mock_server)
            .await;

        let ok = wiremock::ResponseTemplate::new(StatusCode::OK);
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(path("/ok"))
            .respond_with(ok)
            .mount(&mock_server)
            .await;

        let client = ClientBuilder::builder()
            .max_redirects(0_usize)
            .build()
            .client()
            .unwrap();

        let res = client.check(redirect_uri.clone()).await.unwrap();
        assert!(res.status().is_error());

        let client = ClientBuilder::builder()
            .max_redirects(1_usize)
            .build()
            .client()
            .unwrap();

        let res = client.check(redirect_uri).await.unwrap();
        assert!(res.status().is_success());
    }

    #[tokio::test]
    async fn test_limit_max_redirects() {
        let mock_server = wiremock::MockServer::start().await;

        // Set up permanent redirect loop
        let template = wiremock::ResponseTemplate::new(StatusCode::PERMANENT_REDIRECT)
            .insert_header("Location", mock_server.uri().as_str());
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(template)
            .mount(&mock_server)
            .await;

        let client = ClientBuilder::builder()
            .max_redirects(0_usize)
            .build()
            .client()
            .unwrap();

        let res = client.check(mock_server.uri()).await.unwrap();
        assert!(res.status().is_error());
    }

    #[tokio::test]
    async fn test_unsupported_scheme() {
        let examples = vec![
            "ftp://example.com",
            "gopher://example.com",
            "slack://example.com",
        ];

        for example in examples {
            let client = ClientBuilder::builder().build().client().unwrap();
            let res = client.check(example).await.unwrap();
            assert!(res.status().is_unsupported());
        }
    }

    #[tokio::test]
    async fn test_chain() {
        use reqwest::Request;

        #[derive(Debug)]
        struct ExampleHandler();

        #[async_trait]
        impl Handler<Request, Status> for ExampleHandler {
            async fn handle(&mut self, _: Request) -> ChainResult<Request, Status> {
                ChainResult::Done(Status::Excluded)
            }
        }

        let chain = RequestChain::new(vec![Box::new(ExampleHandler {})]);

        let client = ClientBuilder::builder()
            .plugin_request_chain(chain)
            .build()
            .client()
            .unwrap();

        let result = client.check("http://example.com");
        let res = result.await.unwrap();
        assert_eq!(res.status(), &Status::Excluded);
    }
}
