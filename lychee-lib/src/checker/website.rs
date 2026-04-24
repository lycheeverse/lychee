use crate::{
    BasicAuthCredentials, ErrorKind, FileType, FragmentCheckerOptions, Status, Uri,
    chain::{Chain, ChainResult, ClientRequestChains, Handler, RequestChain},
    quirks::Quirks,
    ratelimit::HostPool,
    retry::RetryExt,
    types::{
        redirect_history::{RedirectHistory, Redirects},
        uri::github::GithubUri,
    },
    utils::fragment_checker::{FragmentChecker, FragmentInput},
};
use async_trait::async_trait;
use http::{Method, StatusCode};
use octocrab::Octocrab;
use reqwest::{Request, header::CONTENT_TYPE};
use std::{borrow::Cow, collections::HashSet, path::Path, sync::Arc, time::Duration};
use url::Url;

#[derive(Debug, Clone)]
pub(crate) struct WebsiteChecker {
    /// Request method used for making requests.
    method: reqwest::Method,

    /// GitHub client used for requests.
    github_client: Option<Octocrab>,

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
}

impl WebsiteChecker {
    /// Get a reference to `HostPool`
    #[must_use]
    pub(crate) fn host_pool(&self) -> Arc<HostPool> {
        self.host_pool.clone()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        method: reqwest::Method,
        retry_wait_time: Duration,
        redirect_history: RedirectHistory,
        max_retries: u64,
        accepted: HashSet<StatusCode>,
        github_client: Option<Octocrab>,
        require_https: bool,
        plugin_request_chain: RequestChain,
        fragment_checker_options: FragmentCheckerOptions,
        host_pool: Arc<HostPool>,
    ) -> Self {
        Self {
            method,
            github_client,
            plugin_request_chain,
            redirect_history,
            max_retries,
            retry_wait_time,
            accepted,
            require_https,
            fragment_checker_options,
            fragment_checker: FragmentChecker::new(),
            host_pool,
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

    /// Check a URI using [reqwest](https://github.com/seanmonstar/reqwest).
    async fn check_default(&self, request: Request) -> Status {
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
    pub(crate) async fn check_website(
        &self,
        uri: &Uri,
        credentials: Option<BasicAuthCredentials>,
    ) -> (Status, Option<Redirects>) {
        let default_chain: RequestChain = Chain::new(vec![
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
        let request = self.host_pool.build_request(self.method.clone(), uri);

        let request = match request {
            Ok(r) => r,
            Err(e) => return e.into(),
        };

        let status = ClientRequestChains::new(vec![&self.plugin_request_chain, default_chain])
            .traverse(request)
            .await;

        self.handle_github(status, uri).await
    }

    // Pull out the heavy machinery in case of a failed normal request.
    // This could be a GitHub URL and we ran into the rate limiter.
    // TODO: We should try to parse the URI as GitHub URI first (Lucius, Jan 2023)
    async fn handle_github(&self, status: Status, uri: &Uri) -> Status {
        if status.is_success() {
            return status;
        }

        if let Ok(github_uri) = GithubUri::try_from(uri) {
            let status = self.check_github(github_uri).await;
            if status.is_success() {
                return status;
            }
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
        let Some(client) = &self.github_client else {
            return ErrorKind::MissingGitHubToken.into();
        };
        let repo = match client.repos(&uri.owner, &uri.repo).get().await {
            Ok(repo) => repo,
            Err(e) => return ErrorKind::GithubRequest(Box::new(e)).into(),
        };
        if let Some(true) = repo.private {
            return Status::Ok(StatusCode::OK);
        } else if let Some(endpoint) = uri.endpoint {
            return ErrorKind::InvalidGithubUrl(format!("{}/{}/{endpoint}", uri.owner, uri.repo))
                .into();
        }
        Status::Ok(StatusCode::OK)
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
    use std::{sync::Arc, time::Duration};

    use http::Method;
    use octocrab::Octocrab;

    use crate::{
        FragmentCheckerOptions, Uri,
        chain::RequestChain,
        checker::website::WebsiteChecker,
        ratelimit::HostPool,
        types::{
            DEFAULT_ACCEPTED_STATUS_CODES, redirect_history::RedirectHistory,
            uri::github::GithubUri,
        },
    };

    /// Test GitHub client integration.
    /// This prevents a regression of <https://github.com/lycheeverse/lychee/issues/2024>
    #[tokio::test]
    async fn test_github_client_integration() {
        let client = Octocrab::builder().personal_token("dummy").build().unwrap();
        let uri =
            GithubUri::try_from(Uri::try_from("https://github.com/lycheeverse/lychee").unwrap())
                .unwrap();

        let status = get_checker(client).check_github(uri).await;

        // Because of the invalid authentication token the request failed.
        // But we proved how we could build a client and perform a request.
        assert!(status.is_error());
    }

    fn get_checker(client: Octocrab) -> WebsiteChecker {
        let host_pool = HostPool::default();
        WebsiteChecker::new(
            Method::GET,
            Duration::ZERO,
            RedirectHistory::new(),
            0,
            DEFAULT_ACCEPTED_STATUS_CODES.clone(),
            Some(client),
            false,
            RequestChain::default(),
            FragmentCheckerOptions::default(),
            Arc::new(host_pool),
        )
    }
}
