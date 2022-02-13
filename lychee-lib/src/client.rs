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
use std::{collections::HashSet, time::Duration};

use check_if_email_exists::{check_email, CheckEmailInput, Reachable};
use http::{
    header::{HeaderMap, HeaderValue},
    StatusCode,
};
use octocrab::Octocrab;
use regex::RegexSet;
use reqwest::header;
use secrecy::{ExposeSecret, SecretString};
use tokio::time::sleep;
use typed_builder::TypedBuilder;

use crate::{
    filter::{Excludes, Filter, Includes},
    quirks::Quirks,
    types::{mail, GithubUri},
    ErrorKind, Request, Response, Result, Status, Uri,
};

/// Default number of redirects before a request is deemed as failed, 5.
pub const DEFAULT_MAX_REDIRECTS: usize = 5;
/// Default number of retries before a request is deemed as failed, 3.
pub const DEFAULT_MAX_RETRIES: u64 = 3;
/// Default wait time in seconds between requests, 1.
pub const DEFAULT_RETRY_WAIT_TIME: u64 = 1;
/// Default timeout in seconds before a request is deemed as failed, 20.
pub const DEFAULT_TIMEOUT: usize = 20;
/// Default user agent, `lychee-<PKG_VERSION>`.
pub const DEFAULT_USER_AGENT: &str = concat!("lychee/", env!("CARGO_PKG_VERSION"));

/// Builder for [`Client`].
///
/// See crate-level documentation for usage example.
#[derive(TypedBuilder, Debug, Clone)]
#[builder(field_defaults(default, setter(into)))]
#[builder(builder_method_doc = "
Create a builder for building `ClientBuilder`.

On the builder call, call methods with same name as its fields to set their values.

Finally, call `.build()` to create the instance of `ClientBuilder`.
")]
pub struct ClientBuilder {
    /// Optional GitHub token used for GitHub links.
    ///
    /// This allows much more request before getting rate-limited.
    ///
    /// ## Rate-limiting Defaults
    ///
    /// As of Feb 2022, it's 60 per hour without GitHub token v.s.
    /// 5000 per hour with token.
    github_token: Option<SecretString>,
    /// Links matching this set of regular expressions are **always** checked.
    ///
    /// This has higher precedence over [`ClientBuilder::excludes`], **but**
    /// has lower precedence over any other `exclude_` fields or
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
    /// ## IPv4
    ///
    /// The private address ranges are defined in [IETF RFC 1918] and include:
    ///
    ///  - `10.0.0.0/8`
    ///  - `172.16.0.0/12`
    ///  - `192.168.0.0/16`
    ///
    /// ## IPv6
    ///
    /// The address is a unique local address (`fc00::/7`).
    ///
    /// This property is defined in [IETF RFC 4193].
    ///
    /// ## Note
    ///
    /// Unicast site-local network was defined in [IETF RFC 4291], but was fully deprecated in
    /// [IETF RFC 3879]. So it is **NOT** considered as private on this purpose.
    ///
    /// [IETF RFC 1918]: https://tools.ietf.org/html/rfc1918
    /// [IETF RFC 4193]: https://tools.ietf.org/html/rfc4193
    /// [IETF RFC 4291]: https://tools.ietf.org/html/rfc4291
    /// [IETF RFC 3879]: https://tools.ietf.org/html/rfc3879
    exclude_private_ips: bool,
    /// When `true`, exclude link-local IPs.
    ///
    /// ## IPv4
    ///
    /// The address is `169.254.0.0/16`.
    ///
    /// This property is defined by [IETF RFC 3927].
    ///
    /// ## IPv6
    ///
    /// The address is a unicast address with link-local scope,  as defined in [RFC 4291].
    ///
    /// A unicast address has link-local scope if it has the prefix `fe80::/10`, as per [RFC 4291 section 2.4].
    ///
    /// [IETF RFC 3927]: https://tools.ietf.org/html/rfc3927
    /// [RFC 4291]: https://tools.ietf.org/html/rfc4291
    /// [RFC 4291 section 2.4]: https://tools.ietf.org/html/rfc4291#section-2.4
    exclude_link_local_ips: bool,
    /// When `true`, exclude loopback IP addresses.
    ///
    /// ## IPv4
    ///
    /// This is a loopback address (`127.0.0.0/8`).
    ///
    /// This property is defined by [IETF RFC 1122].
    ///
    /// ## IPv6
    ///
    /// This is the loopback address (`::1`), as defined in [IETF RFC 4291 section 2.5.3].
    ///
    /// [IETF RFC 1122]: https://tools.ietf.org/html/rfc1122
    /// [IETF RFC 4291 section 2.5.3]: https://tools.ietf.org/html/rfc4291#section-2.5.3
    exclude_loopback_ips: bool,
    /// When `true`, don't check mail addresses.
    exclude_mail: bool,
    /// Maximum number of redirects per request before returning an error.
    #[builder(default = DEFAULT_MAX_REDIRECTS)]
    max_redirects: usize,
    /// Maximum number of retries per request before returning an error.
    #[builder(default = DEFAULT_MAX_RETRIES)]
    max_retries: u64,
    /// User-agent used for checking links.
    ///
    /// *NOTE*: This may be helpful for bypassing certain firewalls.
    // Faking the user agent is necessary for some websites, unfortunately.
    // Otherwise we get a 403 from the firewall (e.g. Sucuri/Cloudproxy on ldra.com).
    #[builder(default_code = "String::from(DEFAULT_USER_AGENT)")]
    user_agent: String,
    /// When `true`, accept invalid SSL certificates.
    ///
    /// ## Warning
    ///
    /// You should think very carefully before using this method. If
    /// invalid certificates are trusted, any certificate for any site
    /// will be trusted for use. This includes expired certificates. This
    /// introduces significant vulnerabilities, and should only be used
    /// as a last resort.
    allow_insecure: bool,
    /// When non-empty, only links with matched URI schemes are checked.
    /// Otherwise, this has no effect.
    schemes: HashSet<String>,
    /// Sets the default [headers] for every request. See also [here].
    ///
    /// This allows working around validation issues on some websites.
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
    /// Response timeout per request.
    timeout: Option<Duration>,
    /// Requires using HTTPS when it's available.
    ///
    /// This would treat unencrypted links as errors when HTTPS is avaliable.
    require_https: bool,
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
    /// - The user-agent is invalid.
    /// - The request client cannot be created.
    ///   See [here](https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html#errors).
    /// - The Github client cannot be created.
    pub fn client(self) -> Result<Client> {
        let Self {
            github_token,
            includes,
            excludes,
            user_agent,
            schemes,
            custom_headers: mut headers,
            method,
            accepted,
            ..
        } = self;

        headers.insert(
            header::USER_AGENT,
            HeaderValue::from_str(&user_agent).map_err(ErrorKind::InvalidHeader)?,
        );

        headers.insert(
            header::TRANSFER_ENCODING,
            HeaderValue::from_static("chunked"),
        );

        let builder = reqwest::ClientBuilder::new()
            .gzip(true)
            .default_headers(headers)
            .danger_accept_invalid_certs(self.allow_insecure)
            .redirect(reqwest::redirect::Policy::limited(self.max_redirects));

        let reqwest_client = (match self.timeout {
            Some(t) => builder.timeout(t),
            None => builder,
        })
        .build()
        .map_err(ErrorKind::BuildRequestClient)?;

        let github_client = match github_token.as_ref().map(ExposeSecret::expose_secret) {
            Some(token) if !token.is_empty() => Some(
                Octocrab::builder()
                    .personal_token(token.clone())
                    .build()
                    .map_err(ErrorKind::Github)?,
            ),
            _ => None,
        };

        let filter = Filter {
            includes: includes.map(|regex| Includes { regex }),
            excludes: excludes.map(|regex| Excludes { regex }),
            schemes,
            // exclude_all_private option turns on all "private" excludes,
            // including private IPs, link-local IPs and loopback IPs
            exclude_private_ips: self.exclude_all_private || self.exclude_private_ips,
            exclude_link_local_ips: self.exclude_all_private || self.exclude_link_local_ips,
            exclude_loopback_ips: self.exclude_all_private || self.exclude_loopback_ips,
            exclude_mail: self.exclude_mail,
        };

        let quirks = Quirks::default();

        Ok(Client {
            reqwest_client,
            github_client,
            filter,
            max_retries: self.max_retries,
            method,
            accepted,
            require_https: self.require_https,
            quirks,
        })
    }
}

/// Handles incoming requests and returns responses.
///
/// See [`ClientBuilder`] which contains sane defaults for all configuration options.
#[derive(Debug, Clone)]
pub struct Client {
    /// Underlying `reqwest` client instance that handles the HTTP requests.
    reqwest_client: reqwest::Client,
    /// Github client.
    github_client: Option<Octocrab>,
    /// Rules to decided whether each link would be checked or ignored.
    filter: Filter,
    /// Maximum number of retries per request before returning an error.
    max_retries: u64,
    /// HTTP method used for requests, e.g. `GET` or `HEAD`.
    method: reqwest::Method,
    /// Set of accepted return codes / status codes.
    ///
    /// Unmatched return codes/ status codes are deemed as errors.
    accepted: Option<HashSet<StatusCode>>,
    /// Requires using HTTPS when it's available.
    ///
    /// This would treat unecrypted links as errors when HTTPS is avaliable.
    require_https: bool,
    /// Override behaviors for certain known issues with special URIs.
    quirks: Quirks,
}

impl Client {
    /// Check a single request
    ///
    /// # Errors
    ///
    /// This returns an `Err` if
    /// - `request` is invalid.
    /// - The URI of the request is invalid.
    /// - Encrypted connection for a HTTP URL is available but unused.
    ///   (Only checked when `Client::require_https` is `true`.)
    pub async fn check<T, E>(&self, request: T) -> Result<Response>
    where
        Request: TryFrom<T, Error = E>,
        ErrorKind: From<E>,
    {
        let Request { uri, source, .. } = request.try_into()?;

        // TODO: Allow filtering based on element and attribute
        let status = if self.filter.is_excluded(&uri) {
            Status::Excluded
        } else if uri.is_file() {
            self.check_file(&uri).await
        } else if uri.is_mail() {
            self.check_mail(&uri).await
        } else {
            match self.check_website(&uri).await {
                Status::Ok(code) if self.require_https && uri.scheme() == "http" => {
                    let mut https_uri = uri.clone();
                    https_uri
                        .set_scheme("https")
                        .map_err(|_| ErrorKind::InvalidURI(uri.clone()))?;
                    if self.check_website(&https_uri).await.is_success() {
                        Status::Error(Box::new(ErrorKind::InsecureURL(https_uri)))
                    } else {
                        Status::Ok(code)
                    }
                }
                s => s,
            }
        };

        Ok(Response::new(uri, status, source))
    }

    /// Returns whether the given `uri` should be ignored from checking.
    #[must_use]
    pub fn is_excluded(&self, uri: &Uri) -> bool {
        self.filter.is_excluded(uri)
    }

    /// Checks the given `uri` of a website.
    ///
    /// Here `uri` must has either `http` or `https` scheme.
    pub async fn check_website(&self, uri: &Uri) -> Status {
        let mut retries: u64 = 0;
        let mut wait = DEFAULT_RETRY_WAIT_TIME;

        let mut status = self.check_default(uri).await;
        while retries < self.max_retries {
            if status.is_success() {
                return status;
            }
            sleep(Duration::from_secs(wait)).await;
            retries += 1;
            wait *= 2;
            status = self.check_default(uri).await;
        }

        // Pull out the heavy machinery in case of a failed normal request.
        // This could be a GitHub URL and we ran into the rate limiter.
        if let Some(github_uri) = uri.gh_org_and_repo() {
            return self.check_github(github_uri).await;
        }

        status
    }

    /// Check a `uri` hosted on `GitHub` via the GitHub API.
    ///
    /// # Caveats
    ///
    /// Files inside private repositories won't get checked and instead would
    /// be reported as valid if the repository itself is reachable through the
    /// API.
    ///
    /// A better approach would be to download the file through the API or
    /// clone the repo, but we chose the pragmatic approach.
    async fn check_github(&self, uri: GithubUri) -> Status {
        let client = match &self.github_client {
            Some(client) => client,
            None => return ErrorKind::MissingGitHubToken.into(),
        };
        let repo = match client.repos(&uri.owner, &uri.repo).get().await {
            Ok(repo) => repo,
            Err(e) => return ErrorKind::Github(e).into(),
        };
        if let Some(true) = repo.private {
            // The private repo exists. Assume a given endpoint exists as well
            // (e.g. `issues` in `github.com/org/private/issues`). This is not
            // always the case but simplifies the check.
            return Status::Ok(StatusCode::OK);
        } else if uri.endpoint.is_some() {
            // The URI returned a non-200 status code from a normal request and
            // now we find that this public repo is reachable through the API,
            // so that must mean the full URI (which includes the additional
            // endpoint) must be invalid.
            return ErrorKind::InvalidGitRepo(format!("{}/{}", uri.owner, uri.repo)).into();
        }
        // Found public repo without endpoint
        Status::Ok(StatusCode::OK)
    }

    /// Check a URI using [reqwest](https://github.com/seanmonstar/reqwest).
    async fn check_default(&self, uri: &Uri) -> Status {
        let request = match self
            .reqwest_client
            .request(self.method.clone(), uri.as_str())
            .build()
        {
            Ok(r) => r,
            Err(e) => return e.into(),
        };

        let request = self.quirks.apply(request);

        match self.reqwest_client.execute(request).await {
            Ok(ref response) => Status::new(response, self.accepted.clone()),
            Err(e) => e.into(),
        }
    }

    /// Check a `file` URI.
    pub async fn check_file(&self, uri: &Uri) -> Status {
        if let Ok(path) = uri.url.to_file_path() {
            if path.exists() {
                return Status::Ok(StatusCode::OK);
            }
        }
        ErrorKind::InvalidFilePath(uri.clone()).into()
    }

    /// Check a mail address, or equivalently a `mailto` URI.
    pub async fn check_mail(&self, uri: &Uri) -> Status {
        let input = CheckEmailInput::new(vec![uri.as_str().to_owned()]);
        let result = &(check_email(&input).await)[0];

        if let Reachable::Invalid = result.is_reachable {
            ErrorKind::UnreachableEmailAddress(uri.clone(), mail::error_from_output(result)).into()
        } else {
            Status::Ok(StatusCode::OK)
        }
    }
}

/// A convenience function to check a single URI.
///
/// This provides the simplest link check utility without having to create a [`Client`].
/// For more complex scenarios, see documentation of [`ClientBuilder`] instead.
///
/// # Errors
///
/// Returns an `Err` if:
/// - The request client cannot be built (see [`ClientBuilder::client`] for failure cases).
/// - The request cannot be checked (see [`Client::check`] for failure cases).
pub async fn check<T, E>(request: T) -> Result<Response>
where
    Request: TryFrom<T, Error = E>,
    ErrorKind: From<E>,
{
    let client = ClientBuilder::builder().build().client()?;
    Ok(client.check(request).await?)
}

#[cfg(test)]
mod test {
    use std::{
        fs::File,
        time::{Duration, Instant},
    };

    use http::{header::HeaderMap, StatusCode};
    use reqwest::header;
    use tempfile::tempdir;

    use super::ClientBuilder;
    use crate::{mock_server, test_utils::get_mock_client_response, Uri};

    #[tokio::test]
    async fn test_nonexistent() {
        let mock_server = mock_server!(StatusCode::NOT_FOUND);
        let res = get_mock_client_response(mock_server.uri()).await;

        assert!(res.status().is_failure());
    }

    #[tokio::test]
    async fn test_nonexistent_with_path() {
        let res = get_mock_client_response("http://127.0.0.1/invalid").await;
        assert!(res.status().is_failure());
    }

    #[tokio::test]
    async fn test_exponential_backoff() {
        let mock_server = mock_server!(StatusCode::NOT_FOUND);

        let start = Instant::now();
        let res = get_mock_client_response(mock_server.uri()).await;
        let end = start.elapsed();

        assert!(res.status().is_failure());

        // on slow connections, this might take a bit longer than nominal backed-off timeout (7 secs)
        assert!(end.as_secs() >= 7);
        assert!(end.as_secs() <= 8);
    }

    #[tokio::test]
    async fn test_github() {
        let res = get_mock_client_response("https://github.com/lycheeverse/lychee").await;
        assert!(res.status().is_success());
    }

    #[tokio::test]
    async fn test_github_nonexistent_repo() {
        let res = get_mock_client_response("https://github.com/lycheeverse/not-lychee").await;
        assert!(res.status().is_failure());
    }

    #[tokio::test]
    async fn test_github_nonexistent_file() {
        let res = get_mock_client_response(
            "https://github.com/lycheeverse/lychee/blob/master/NON_EXISTENT_FILE.md",
        )
        .await;
        assert!(res.status().is_failure());
    }

    #[tokio::test]
    async fn test_youtube() {
        // This is applying a quirk. See the quirks module.
        let res = get_mock_client_response("https://www.youtube.com/watch?v=NlKuICiT470&list=PLbWDhxwM_45mPVToqaIZNbZeIzFchsKKQ&index=7").await;
        assert!(res.status().is_success());

        let res = get_mock_client_response("https://www.youtube.com/watch?v=invalidNlKuICiT470&list=PLbWDhxwM_45mPVToqaIZNbZeIzFchsKKQ&index=7").await;
        assert!(res.status().is_failure());
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

        assert!(res.status().is_failure());

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
    async fn test_exclude_mail() {
        let client = ClientBuilder::builder()
            .exclude_mail(false)
            .exclude_all_private(true)
            .build()
            .client()
            .unwrap();
        assert!(!client.is_excluded(&Uri {
            url: "mailto://mail@example.com".try_into().unwrap()
        }));

        let client = ClientBuilder::builder()
            .exclude_mail(true)
            .exclude_all_private(true)
            .build()
            .client()
            .unwrap();
        assert!(client.is_excluded(&Uri {
            url: "mailto://mail@example.com".try_into().unwrap()
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
        assert!(res.status().is_failure());
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
}
