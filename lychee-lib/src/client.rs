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
use hubcaps::{Credentials, Github};
use regex::RegexSet;
use reqwest::header;
use tokio::time::sleep;
use typed_builder::TypedBuilder;

use crate::{
    filter::{Excludes, Filter, Includes},
    quirks::Quirks,
    types::GithubUri,
    ErrorKind, Request, Response, Result, Status, Uri,
};

/// Default lychee user agent
pub const DEFAULT_USER_AGENT: &str = concat!("lychee/", env!("CARGO_PKG_VERSION"));
/// Number of redirects until a request gets declared as failed
pub const DEFAULT_MAX_REDIRECTS: usize = 5;
/// Number of retries until a request gets declared as failed
pub const DEFAULT_MAX_RETRIES: u64 = 3;
/// Wait time in seconds between requests (will be doubled after every failure)
pub const DEFAULT_RETRY_WAIT_TIME: u64 = 1;
/// Total timeout per request until a request gets declared as failed
pub const DEFAULT_TIMEOUT: usize = 20;

/// Handles incoming requests and returns responses. Usually you would not
/// initialize a `Client` yourself, but use the `ClientBuilder` because it
/// provides sane defaults for all configuration options.
#[derive(Debug, Clone)]
pub struct Client {
    /// Underlying reqwest client instance that handles the HTTP requests.
    reqwest_client: reqwest::Client,
    /// Github client.
    github_client: Option<Github>,
    /// Filtered domain handling.
    filter: Filter,
    /// Maximum number of retries
    max_retries: u64,
    /// Default request HTTP method to use.
    method: reqwest::Method,
    /// The set of accepted HTTP status codes for valid URIs.
    accepted: Option<HashSet<StatusCode>>,
    /// Require HTTPS URL when it's available.
    require_https: bool,
    /// Override behavior for certain known issues with URIs.
    quirks: Quirks,
}

/// A link checker using an API token for Github links
/// otherwise a normal HTTP client.
#[allow(unreachable_pub)]
#[derive(TypedBuilder, Debug)]
#[builder(field_defaults(default, setter(into)))]
pub struct ClientBuilder {
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
    #[builder(default = DEFAULT_MAX_REDIRECTS)]
    max_redirects: usize,
    /// Maximum number of retries before returning error
    #[builder(default = DEFAULT_MAX_RETRIES)]
    max_retries: u64,
    /// User agent used for checking links
    // Faking the user agent is necessary for some websites, unfortunately.
    // Otherwise we get a 403 from the firewall (e.g. Sucuri/Cloudproxy on ldra.com).
    #[builder(default_code = "String::from(DEFAULT_USER_AGENT)")]
    user_agent: String,
    /// Ignore SSL errors
    allow_insecure: bool,
    /// Set of allowed URI schemes (e.g. https, http).
    /// This excludes all links from checking, which
    /// don't specify any of these schemes in the URL.
    schemes: HashSet<String>,
    /// Map of headers to send to each resource.
    /// This allows working around validation issues
    /// on some websites.
    custom_headers: HeaderMap,
    /// Request method (e.g. `GET` or `HEAD`)
    #[builder(default = reqwest::Method::GET)]
    method: reqwest::Method,
    /// Set of accepted return codes / status codes
    accepted: Option<HashSet<StatusCode>>,
    /// Response timeout per request
    timeout: Option<Duration>,
    /// Treat HTTP links as errors when HTTPS is available
    require_https: bool,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl ClientBuilder {
    fn build_filter(&self) -> Filter {
        let includes = self.includes.clone().map(|regex| Includes { regex });
        let excludes = self.excludes.clone().map(|regex| Excludes { regex });
        let schemes = self.schemes.clone();

        Filter {
            includes,
            excludes,
            schemes,
            // exclude_all_private option turns on all "private" excludes,
            // including private IPs, link-local IPs and loopback IPs
            exclude_private_ips: self.exclude_all_private || self.exclude_private_ips,
            exclude_link_local_ips: self.exclude_all_private || self.exclude_link_local_ips,
            exclude_loopback_ips: self.exclude_all_private || self.exclude_loopback_ips,
            exclude_mail: self.exclude_mail,
        }
    }

    /// The build method instantiates the client.
    ///
    /// # Errors
    ///
    /// Returns an `Err` if:
    /// - The user agent cannot be parsed
    /// - The request client cannot be created
    /// - The Github client cannot be created
    pub fn client(&self) -> Result<Client> {
        let mut headers = self.custom_headers.clone();
        headers.insert(header::USER_AGENT, HeaderValue::from_str(&self.user_agent)?);
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
        .build()?;

        let github_token = match self.github_token {
            Some(ref token) if !token.is_empty() => Some(Github::new(
                self.user_agent.clone(),
                Credentials::Token(token.clone()),
            )?),
            _ => None,
        };

        let filter = self.build_filter();

        let quirks = Quirks::default();

        Ok(Client {
            reqwest_client,
            github_client: github_token,
            filter,
            max_retries: self.max_retries,
            method: self.method.clone(),
            accepted: self.accepted.clone(),
            require_https: self.require_https,
            quirks,
        })
    }
}

impl Client {
    /// Check a single request
    ///
    /// # Errors
    ///
    /// This returns an `Err` if
    /// - The request cannot be parsed
    /// - An HTTP website with an invalid URI format gets checked
    pub async fn check<T, E>(&self, request: T) -> Result<Response>
    where
        Request: TryFrom<T, Error = E>,
        ErrorKind: From<E>,
    {
        let Request {
            uri,
            source,
            element: _element,
            attribute: _attribute,
        } = request.try_into()?;

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
                        .url
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

    /// Check if the given URI is filtered by the client
    #[must_use]
    pub fn filtered(&self, uri: &Uri) -> bool {
        self.filter.is_excluded(uri)
    }

    /// Check a website URI
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
        // This could be a Github URL and we ran into the rate limiter.
        if let Some(github_uri) = uri.gh_org_and_repo() {
            return self.check_github(github_uri).await;
        }

        status
    }

    /// Check a URI using the Github API.
    ///
    /// Caveat: Files inside private repositories won't get checked and instead
    /// be reported as valid if the repository itself is reachable through the
    /// API. A better approach would be to download the file through the API or
    /// clone the repo, but we chose the pragmatic approach.
    async fn check_github(&self, uri: GithubUri) -> Status {
        let client = match &self.github_client {
            Some(client) => client,
            None => return ErrorKind::MissingGitHubToken.into(),
        };
        let repo = match client.repo(uri.owner, uri.repo).get().await {
            Ok(repo) => repo,
            Err(e) => return ErrorKind::GithubError(Some(e)).into(),
        };
        if repo.private {
            // The private repo exists. Assume a given endpoint exists as well
            // (e.g. `issues` in `github.com/org/private/issues`). This is not
            // always the case but simplifies the check.
            return Status::Ok(StatusCode::OK);
        } else if uri.endpoint.is_some() {
            // The URI returned a non-200 status code from a normal request and
            // now we find that this public repo is reachable through the API,
            // so that must mean the full URI (which includes the additional
            // endpoint) must be invalid.
            return ErrorKind::GithubError(None).into();
        }
        // Found public repo without endpoint
        Status::Ok(StatusCode::OK)
    }

    /// Check a URI using [reqwest](https://github.com/seanmonstar/reqwest)
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

    /// Check a file URI
    pub async fn check_file(&self, uri: &Uri) -> Status {
        if let Ok(path) = uri.url.to_file_path() {
            if path.exists() {
                return Status::Ok(StatusCode::OK);
            }
        }
        ErrorKind::InvalidFilePath(uri.clone()).into()
    }

    /// Check a mail address
    pub async fn check_mail(&self, uri: &Uri) -> Status {
        let input = CheckEmailInput::new(vec![uri.as_str().to_owned()]);
        let result = &(check_email(&input).await)[0];

        if let Reachable::Invalid = result.is_reachable {
            ErrorKind::UnreachableEmailAddress(uri.clone()).into()
        } else {
            Status::Ok(StatusCode::OK)
        }
    }
}

/// A convenience function to check a single URI
/// This is the most simple link check and avoids having to create a client manually.
/// For more complex scenarios, look into using the [`ClientBuilder`] instead.
#[allow(clippy::missing_errors_doc)]
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
        assert!(!client.filtered(&Uri {
            url: "mailto://mail@example.org".try_into().unwrap()
        }));

        let client = ClientBuilder::builder()
            .exclude_mail(true)
            .exclude_all_private(true)
            .build()
            .client()
            .unwrap();
        assert!(client.filtered(&Uri {
            url: "mailto://mail@example.org".try_into().unwrap()
        }));
    }

    #[tokio::test]
    async fn test_require_https() {
        let client = ClientBuilder::builder().build().client().unwrap();
        let res = client.check("http://example.org").await.unwrap();
        assert!(res.status().is_success());

        // Same request will fail if HTTPS is required
        let client = ClientBuilder::builder()
            .require_https(true)
            .build()
            .client()
            .unwrap();
        let res = client.check("http://example.org").await.unwrap();
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
