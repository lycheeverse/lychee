use crate::{
    BasicAuthExtractor, ErrorKind, FileType, FragmentCheckerOptions, Methods, Status, Uri,
    chain::{Chain, ChainResult, ClientRequestChains, Handler, RequestChain},
    checker::github::{GitHubChecker, request::GitHubRequestRewriter},
    quirks::Quirks,
    ratelimit::HostPool,
    retry::RetryExt,
    types::redirect_history::{RedirectHistory, Redirects},
    utils::fragment_checker::{FragmentChecker, FragmentInput},
};
use async_trait::async_trait;
use http::{Method, StatusCode};
use reqwest::{Request, header::CONTENT_TYPE};
use secrecy::SecretString;
use std::{borrow::Cow, collections::HashSet, path::Path, sync::Arc, time::Duration};
use url::Url;

#[derive(Debug, Clone)]
pub(crate) struct WebsiteChecker {
    /// Request methods used for making requests, in order of preference.
    ///
    /// Some servers don't handle certain methods properly (e.g. `HEAD`
    /// requests), so lychee can be configured to try multiple methods in order
    /// and return the first successful one.
    methods: Methods,

    /// GitHub-specific README checks and API fallback.
    github_checker: GitHubChecker,

    /// The chain of plugins to be executed on each request.
    plugin_request_chain: RequestChain,

    /// Maximum number of retries per request before returning an error.
    max_retries: u64,

    /// Initial wait time between retries of failed requests. This doubles after
    /// each failure.
    retry_wait_time: Duration,

    /// Set of accepted return codes / status codes.
    ///
    /// Unmatched return codes/ status codes are deemed as errors.
    accepted: HashSet<StatusCode>,

    /// Requires using HTTPS when it's available.
    ///
    /// This would treat unencrypted links as errors when HTTPS is available.
    require_https: bool,

    /// Controls which fragment types are checked in the response body.
    ///
    /// No fragments are checked if the request method is `HEAD`.
    fragment_checker_options: FragmentCheckerOptions,

    /// Utility for performing fragment checks in HTML files.
    fragment_checker: FragmentChecker,

    /// Keep track of HTTP redirections for reporting
    redirect_history: RedirectHistory,

    /// Optional host pool for per-host rate limiting.
    ///
    /// When present, HTTP requests will be routed through this pool for
    /// rate limiting. When None, requests go directly through `reqwest_client`.
    host_pool: Arc<HostPool>,

    /// Basic auth extractor to obtain credentials from.
    basic_auth: BasicAuthExtractor,
}

impl WebsiteChecker {
    /// Get a reference to `HostPool`
    #[must_use]
    pub(crate) fn host_pool(&self) -> Arc<HostPool> {
        self.host_pool.clone()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        methods: Methods,
        retry_wait_time: Duration,
        redirect_history: RedirectHistory,
        max_retries: u64,
        accepted: HashSet<StatusCode>,
        github_token: Option<SecretString>,
        require_https: bool,
        plugin_request_chain: RequestChain,
        fragment_checker_options: FragmentCheckerOptions,
        host_pool: Arc<HostPool>,
        basic_auth: BasicAuthExtractor,
    ) -> Self {
        Self {
            methods,
            github_checker: GitHubChecker::new(host_pool.clone(), github_token, accepted.clone()),
            plugin_request_chain,
            redirect_history,
            max_retries,
            retry_wait_time,
            accepted,
            require_https,
            fragment_checker_options,
            fragment_checker: FragmentChecker::new(),
            host_pool,
            basic_auth,
        }
    }

    /// Retry requests up to `max_retries` times
    /// with an exponential backoff.
    /// Note that, in addition, there also is a host-specific backoff
    /// when host-specific rate limiting or errors are detected.
    pub(crate) async fn retry_request(&self, request: Request) -> Status {
        let mut retries: u64 = 0;
        let mut wait_time = self.retry_wait_time;
        let mut status = self.check_default(clone_unwrap(&request)).await;
        while retries < self.max_retries {
            if status.is_success() || !status.should_retry() {
                return status;
            }
            retries += 1;
            tokio::time::sleep(wait_time).await;
            wait_time = wait_time.saturating_mul(2);
            status = self.check_default(clone_unwrap(&request)).await;
        }

        status
    }

    async fn check_github_readme(&self, request: &Request) -> Option<Status> {
        if request.method() != Method::GET || !self.fragment_checker_options.check_anchor_fragments
        {
            return None;
        }

        self.github_checker
            .check_readme(&Uri::from(request.url().clone()))
            .await
    }

    /// Check a URI using [reqwest](https://github.com/seanmonstar/reqwest).
    async fn check_default(&self, request: Request) -> Status {
        if let Some(status) = self.check_github_readme(&request).await {
            return status;
        }

        let method = request.method().clone();
        let request_url = request.url().clone();
        let check_request_fragments = self.fragment_checker_options.any_enabled()
            && method == Method::GET
            // This last part ensures empty and top fragments do not trigger body retrieval.
            && request_url.fragment().is_some_and(|x| !x.is_empty());

        match self
            .host_pool
            .execute_request(request, check_request_fragments)
            .await
        {
            Ok(response) => {
                let status = Status::new(&response, &self.accepted);
                // when `accept=200,429`, `status_code=429` will be treated as success
                // but we are not able the check the fragment since it's inapplicable.
                if let Some(content) = response.text
                    && check_request_fragments
                    && response.status.is_success()
                {
                    let Some(content_type) = response
                        .headers
                        .get(CONTENT_TYPE)
                        .and_then(|header| header.to_str().ok())
                    else {
                        return status;
                    };

                    let file_type = match content_type {
                        ct if ct.starts_with("text/html") => FileType::Html,
                        ct if ct.starts_with("text/markdown") => FileType::Markdown,
                        ct if ct.starts_with("text/plain") => {
                            let path = Path::new(response.url.path());
                            match path.extension() {
                                Some(ext) if ext.eq_ignore_ascii_case("md") => FileType::Markdown,
                                _ => return status,
                            }
                        }
                        _ => return status,
                    };

                    self.check_html_fragment(request_url, status, &content, file_type)
                        .await
                } else {
                    status
                }
            }
            Err(e) => e.into(),
        }
    }

    async fn check_html_fragment(
        &self,
        url: Url,
        status: Status,
        content: &str,
        file_type: FileType,
    ) -> Status {
        match self
            .fragment_checker
            .check(
                FragmentInput {
                    content: Cow::Borrowed(content),
                    file_type,
                },
                &url,
                self.fragment_checker_options,
            )
            .await
        {
            Ok(true) => status,
            Ok(false) => Status::Error(ErrorKind::InvalidFragment(url.into())),
            Err(e) => Status::Error(e),
        }
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
    pub(crate) async fn check_website(&self, uri: &Uri) -> (Status, Option<Redirects>) {
        let credentials = self.basic_auth.matches(uri);

        let default_chain: RequestChain = Chain::new(vec![
            Box::<GitHubRequestRewriter>::default(),
            Box::<Quirks>::default(),
            Box::new(credentials),
            Box::new(self.clone()),
        ]);

        let status = self.check_website_inner(uri, &default_chain).await;
        let status = self.handle_insecure_url(uri, &default_chain, status).await;

        let redirects = self.redirect_history.resolve(&uri.url);
        (status, redirects)
    }

    /// Mark HTTP URLs as insecure, if the user required HTTPS
    /// and the URL is available under HTTPS.
    async fn handle_insecure_url(
        &self,
        uri: &Uri,
        default_chain: &Chain<Request, Status>,
        status: Status,
    ) -> Status {
        if self.require_https
            && uri.scheme() == "http"
            && let Status::Ok(_) = status
            && let Ok(https_uri) = uri.to_https()
        {
            let is_https_available = self
                .check_website_inner(&https_uri, default_chain)
                .await
                .is_success();

            if is_https_available {
                return Status::Error(ErrorKind::InsecureURL(https_uri));
            }
        }

        status
    }

    /// Checks the given URI of a website.
    ///
    /// Unsupported schemes will be ignored
    ///
    /// Note: we use `inner` to improve compile times by avoiding monomorphization
    ///
    /// # Errors
    ///
    /// This returns an `Err` if
    /// - The URI is invalid.
    /// - The request failed.
    /// - The response status code is not accepted.
    async fn check_website_inner(&self, uri: &Uri, default_chain: &RequestChain) -> Status {
        let mut last_status = None;

        // Try each configured method in order and return the first success.
        //
        // Servers that don't accept a given method signal this in many different
        // ways (404, 403, 405, ...), so we deliberately fall back on any error
        // response without inspecting the reason. We also fall back on connection
        // errors, since some servers reject a method by resetting the connection
        // rather than returning a status code.
        //
        // The one exception is timeouts: a timeout is unlikely to be resolved by
        // switching methods (a heavier method such as `GET` would, if anything,
        // take longer than a lighter one such as `HEAD`), and retrying would
        // incur a second, equally long timeout, so we stop early instead.
        for method in self.methods.iter() {
            let status = self
                .check_with_method(method.clone(), uri, default_chain)
                .await;

            if status.is_success() || status.is_timeout() {
                return status;
            }

            last_status = Some(status);
        }

        // `methods` is guaranteed to be non-empty (see `Methods`), so the loop
        // always runs at least once and `last_status` is always `Some` here.
        last_status.expect("Methods is guaranteed to be non-empty")
    }

    /// Build and check a single request for `uri` using the given `method`.
    async fn check_with_method(
        &self,
        method: Method,
        uri: &Uri,
        default_chain: &RequestChain,
    ) -> Status {
        let request = match self.host_pool.build_request(method.clone(), uri) {
            Ok(request) => request,
            Err(e) => return e.into(),
        };

        let status = ClientRequestChains::new(vec![&self.plugin_request_chain, default_chain])
            .traverse(request)
            .await;

        self.github_checker
            .check_after_website_failure(status, uri, &method, self.fragment_checker_options)
            .await
    }
}

/// Clones a `reqwest::Request`.
///
/// # Safety
///
/// This panics if the request cannot be cloned. This should only happen if the
/// request body is a `reqwest` stream. We disable the `stream` feature, so the
/// body should never be a stream.
///
/// See <https://github.com/seanmonstar/reqwest/blob/de5dbb1ab849cc301dcefebaeabdf4ce2e0f1e53/src/async_impl/body.rs#L168>
fn clone_unwrap(request: &Request) -> Request {
    request.try_clone().expect("Failed to clone request: body was a stream, which should be impossible with `stream` feature disabled")
}

#[async_trait]
impl Handler<Request, Status> for WebsiteChecker {
    async fn handle(&mut self, input: Request) -> ChainResult<Request, Status> {
        ChainResult::Done(self.retry_request(input).await)
    }
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::Arc, time::Duration};

    use http::{Method, StatusCode};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method as method_matcher, path as path_matcher},
    };

    use crate::{
        BasicAuthExtractor, ErrorKind, FragmentCheckerOptions, Status, Uri,
        chain::RequestChain,
        checker::{
            github::{GitHubChecker, api::GitHubApi},
            website::WebsiteChecker,
        },
        ratelimit::{HostConfigs, HostPool, RateLimitConfig},
        types::{DEFAULT_ACCEPTED_STATUS_CODES, Methods, redirect_history::RedirectHistory},
    };

    /// Build a checker for the given methods, routing requests through a
    /// `HostPool` that uses the supplied `reqwest::Client` (so tests can control
    /// e.g. the request timeout).
    fn checker_with(methods: Methods, client: reqwest::Client) -> WebsiteChecker {
        let host_pool = HostPool::new(
            RateLimitConfig::default(),
            HostConfigs::default(),
            client,
            std::collections::HashMap::new(),
        );
        WebsiteChecker::new(
            methods,
            Duration::ZERO,
            RedirectHistory::new(),
            0,
            DEFAULT_ACCEPTED_STATUS_CODES.clone(),
            None,
            false,
            RequestChain::default(),
            FragmentCheckerOptions::default(),
            Arc::new(host_pool),
            BasicAuthExtractor::empty(),
        )
    }

    fn checker_with_github_api(api_server: &MockServer) -> WebsiteChecker {
        let client = reqwest::Client::builder()
            .no_proxy()
            .resolve("github.com", *api_server.address())
            .build()
            .unwrap();
        let host_pool = Arc::new(HostPool::new(
            RateLimitConfig::default(),
            HostConfigs::default(),
            client,
            std::collections::HashMap::new(),
        ));

        let github_api = GitHubApi::with_base_url(
            Arc::clone(&host_pool),
            None,
            api_server.uri().parse().unwrap(),
            DEFAULT_ACCEPTED_STATUS_CODES.clone(),
        );

        let mut checker = WebsiteChecker::new(
            Method::GET.into(),
            Duration::ZERO,
            RedirectHistory::new(),
            0,
            DEFAULT_ACCEPTED_STATUS_CODES.clone(),
            None,
            false,
            RequestChain::default(),
            FragmentCheckerOptions {
                check_anchor_fragments: true,
                check_text_fragments: false,
            },
            host_pool,
            BasicAuthExtractor::empty(),
        );
        checker.github_checker = GitHubChecker::with_api(github_api);
        checker
    }

    #[tokio::test]
    async fn test_github_readme_fragment_uses_specialized_api_checker() {
        let api_server = MockServer::start().await;
        Mock::given(method_matcher("GET"))
            .and(path_matcher("/repos/lycheeverse/lychee/readme"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&api_server)
            .await;

        let checker = checker_with_github_api(&api_server);
        let uri = Uri::try_from("https://github.com/lycheeverse/lychee#readme").unwrap();

        let (status, _) = checker.check_website(&uri).await;

        assert!(status.is_success(), "expected success, got {status:?}");
    }

    fn github_readme_uri(server: &MockServer) -> Uri {
        Uri::try_from(format!(
            "http://github.com:{}/lycheeverse/lychee#readme",
            server.address().port()
        ))
        .unwrap()
    }

    #[tokio::test]
    async fn test_github_readme_head_does_not_check_fragments() {
        let server = MockServer::start().await;
        Mock::given(method_matcher("HEAD"))
            .and(path_matcher("/lycheeverse/lychee"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(path_matcher("/repos/lycheeverse/lychee/readme"))
            .respond_with(ResponseTemplate::new(404))
            .expect(0)
            .mount(&server)
            .await;

        let mut checker = checker_with_github_api(&server);
        checker.methods = Method::HEAD.into();
        let (status, _) = checker.check_website(&github_readme_uri(&server)).await;

        assert!(status.is_success(), "expected HEAD success, got {status:?}");
    }

    #[tokio::test]
    async fn test_github_readme_falls_back_from_head_to_get() {
        let server = MockServer::start().await;
        Mock::given(method_matcher("HEAD"))
            .and(path_matcher("/lycheeverse/lychee"))
            .respond_with(ResponseTemplate::new(405))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method_matcher("GET"))
            .and(path_matcher("/repos/lycheeverse/lychee"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method_matcher("GET"))
            .and(path_matcher("/repos/lycheeverse/lychee/readme"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let mut checker = checker_with_github_api(&server);
        checker.methods = Methods::from_str("head,get").unwrap();
        let (status, _) = checker.check_website(&github_readme_uri(&server)).await;

        assert!(
            status.is_success(),
            "expected GET fallback success, got {status:?}"
        );
    }

    #[tokio::test]
    async fn test_github_readme_respects_disabled_anchor_checks() {
        let server = MockServer::start().await;
        Mock::given(method_matcher("GET"))
            .and(path_matcher("/lycheeverse/lychee"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(path_matcher("/repos/lycheeverse/lychee/readme"))
            .respond_with(ResponseTemplate::new(404))
            .expect(0)
            .mount(&server)
            .await;

        let mut checker = checker_with_github_api(&server);
        checker.fragment_checker_options.check_anchor_fragments = false;
        let (status, _) = checker.check_website(&github_readme_uri(&server)).await;

        assert!(
            status.is_success(),
            "expected website success, got {status:?}"
        );
    }

    #[rstest::rstest]
    #[case(0, 1, StatusCode::TOO_MANY_REQUESTS)]
    #[case(1, 2, StatusCode::OK)]
    #[tokio::test]
    async fn test_github_readme_respects_retry_limit(
        #[case] max_retries: u64,
        #[case] expected_requests: u64,
        #[case] expected_status: StatusCode,
    ) {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let server = MockServer::start().await;
        let attempts = AtomicUsize::new(0);
        Mock::given(method_matcher("GET"))
            .and(path_matcher("/repos/lycheeverse/lychee/readme"))
            .respond_with(move |_: &wiremock::Request| {
                let status = if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    429
                } else {
                    200
                };
                ResponseTemplate::new(status)
            })
            .expect(expected_requests)
            .mount(&server)
            .await;
        Mock::given(path_matcher("/repos/lycheeverse/lychee"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;

        let mut checker = checker_with_github_api(&server);
        checker.max_retries = max_retries;
        let (status, _) = checker.check_website(&github_readme_uri(&server)).await;

        assert_eq!(status.code(), Some(expected_status));
    }

    #[tokio::test]
    async fn test_github_missing_readme_is_not_hidden_by_repo_access() {
        let server = MockServer::start().await;
        Mock::given(path_matcher("/repos/lycheeverse/lychee/readme"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(path_matcher("/repos/lycheeverse/lychee"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;

        let mut checker = checker_with_github_api(&server);
        checker.max_retries = 2;
        let (status, _) = checker.check_website(&github_readme_uri(&server)).await;

        assert_eq!(status.code(), Some(StatusCode::NOT_FOUND));
    }

    #[tokio::test]
    async fn test_github_readme_enforces_https() {
        let server = MockServer::start().await;
        Mock::given(method_matcher("GET"))
            .and(path_matcher("/repos/lycheeverse/lychee/readme"))
            .respond_with(ResponseTemplate::new(200))
            // The HTTPS check reuses the cached API response.
            .expect(1)
            .mount(&server)
            .await;

        let mut checker = checker_with_github_api(&server);
        checker.require_https = true;
        let uri = Uri::try_from("http://github.com/lycheeverse/lychee#readme").unwrap();
        let (status, _) = checker.check_website(&uri).await;

        assert_eq!(
            status,
            Status::Error(ErrorKind::InsecureURL(uri.to_https().unwrap()))
        );
    }

    #[tokio::test]
    async fn test_github_fallback_preserves_non_readme_repo_fragment_errors() {
        let api_server = MockServer::start().await;
        let checker = checker_with_github_api(&api_server);
        let uri =
            Uri::try_from("https://github.com/lycheeverse/lychee#non-existent-anchor").unwrap();
        let failure = Status::Error(ErrorKind::InvalidFragment(uri.clone()));

        let status = checker
            .github_checker
            .check_after_website_failure(
                failure,
                &uri,
                &Method::GET,
                checker.fragment_checker_options,
            )
            .await;

        assert_eq!(
            status,
            Status::Error(ErrorKind::InvalidFragment(uri)),
            "expected the original fragment error to be preserved"
        );
    }

    #[tokio::test]
    async fn test_github_fallback_accepts_private_repo_paths() {
        let api_server = MockServer::start().await;
        Mock::given(method_matcher("GET"))
            .and(path_matcher("/repos/lycheeverse/private"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "private": true,
            })))
            .expect(3)
            .mount(&api_server)
            .await;

        for url in [
            "https://github.com/lycheeverse/private/blob/master/missing.md",
            "https://www.github.com/lycheeverse/private/blob/master/missing.md",
            "https://raw.githubusercontent.com/lycheeverse/private/master/missing.md",
        ] {
            let checker = checker_with_github_api(&api_server);
            let uri = Uri::try_from(url).unwrap();
            let status = checker
                .github_checker
                .check_after_website_failure(
                    Status::Error(ErrorKind::RejectedStatusCode(StatusCode::NOT_FOUND)),
                    &uri,
                    &Method::GET,
                    checker.fragment_checker_options,
                )
                .await;

            assert!(
                status.is_success(),
                "expected success for {url}, got {status:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_github_fallback_reports_public_repo_paths_as_invalid() {
        let api_server = MockServer::start().await;
        Mock::given(method_matcher("GET"))
            .and(path_matcher("/repos/lycheeverse/lychee"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "private": false,
            })))
            .expect(1)
            .mount(&api_server)
            .await;

        let checker = checker_with_github_api(&api_server);
        let uri =
            Uri::try_from("https://github.com/lycheeverse/lychee/blob/master/missing.md").unwrap();

        let status = checker
            .github_checker
            .check_after_website_failure(
                Status::Error(ErrorKind::RejectedStatusCode(StatusCode::NOT_FOUND)),
                &uri,
                &Method::GET,
                checker.fragment_checker_options,
            )
            .await;

        assert!(status.is_error(), "expected error, got {status:?}");
        assert_eq!(status.code(), None);
    }

    /// When every configured method fails, the status of the *last* method
    /// attempted is returned (not the first).
    #[tokio::test]
    async fn test_fallback_returns_last_status_when_all_fail() {
        let server = MockServer::start().await;
        Mock::given(method_matcher("HEAD"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method_matcher("GET"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let methods = Methods::from_str("head,get").unwrap();
        let checker = checker_with(methods, reqwest::Client::new());
        let uri = Uri::try_from(server.uri().as_str()).unwrap();

        let (status, _) = checker.check_website(&uri).await;

        assert!(status.is_error());
        // 403 is GET's response: the loop fell back from HEAD and kept GET's
        // status as the final result.
        assert_eq!(status.code(), Some(StatusCode::FORBIDDEN));
    }

    /// A timeout is method-independent, so the fallback loop stops immediately
    /// instead of retrying with the next method (which would incur a second,
    /// equally long timeout).
    #[tokio::test]
    async fn test_timeout_short_circuits_fallback() {
        let server = MockServer::start().await;
        // HEAD hangs long enough to trip the client timeout below.
        Mock::given(method_matcher("HEAD"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
            .mount(&server)
            .await;
        // GET would succeed immediately, so if the loop fell through we'd see
        // success rather than a timeout.
        Mock::given(method_matcher("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();
        let methods = Methods::from_str("head,get").unwrap();
        let checker = checker_with(methods, client);
        let uri = Uri::try_from(server.uri().as_str()).unwrap();

        let (status, _) = checker.check_website(&uri).await;

        assert!(
            status.is_timeout(),
            "expected timeout to short-circuit fallback, got {status:?}"
        );
    }
}
