use crate::{
    BasicAuthCredentials, ErrorKind, Status, Uri,
    chain::{Chain, ChainResult, ClientRequestChains, Handler, RequestChain},
    quirks::Quirks,
    retry::RetryExt,
    types::uri::github::GithubUri,
    utils::fragment_checker::{FragmentChecker, FragmentInput},
};
use async_trait::async_trait;
use http::{Method, StatusCode};
use octocrab::Octocrab;
use reqwest::{Request, Response};
use std::{collections::HashSet, time::Duration};

#[derive(Debug, Clone)]
pub(crate) struct WebsiteChecker {
    /// Request method used for making requests.
    method: reqwest::Method,

    /// The HTTP client used for requests.
    reqwest_client: reqwest::Client,

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

    /// Whether to check the existence of fragments in the response HTML files.
    ///
    /// Will be disabled if the request method is `HEAD`.
    include_fragments: bool,

    /// Utility for performing fragment checks in HTML files.
    fragment_checker: FragmentChecker,
}

impl WebsiteChecker {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        method: reqwest::Method,
        retry_wait_time: Duration,
        max_retries: u64,
        reqwest_client: reqwest::Client,
        accepted: HashSet<StatusCode>,
        github_client: Option<Octocrab>,
        require_https: bool,
        plugin_request_chain: RequestChain,
        include_fragments: bool,
    ) -> Self {
        Self {
            method,
            reqwest_client,
            github_client,
            plugin_request_chain,
            max_retries,
            retry_wait_time,
            accepted,
            require_https,
            include_fragments,
            fragment_checker: FragmentChecker::new(),
        }
    }

    /// Retry requests up to `max_retries` times
    /// with an exponential backoff.
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
        match self.reqwest_client.execute(request).await {
            Ok(response) => {
                let status = Status::new(&response, &self.accepted);
                if self.include_fragments && status.is_success() && method == Method::GET {
                    self.check_html_fragment(status, response).await
                } else {
                    status
                }
            }
            Err(e) => e.into(),
        }
    }

    async fn check_html_fragment(&self, status: Status, response: Response) -> Status {
        let url = response.url().clone();
        match response.text().await {
            Ok(text) => {
                match self
                    .fragment_checker
                    .check(
                        FragmentInput {
                            content: text,
                            file_type: crate::FileType::Html,
                        },
                        &url,
                    )
                    .await
                {
                    Ok(true) => status,
                    Ok(false) => Status::Error(ErrorKind::InvalidFragment(url.into())),
                    Err(e) => Status::Error(e),
                }
            }
            Err(e) => Status::Error(ErrorKind::ReadResponseBody(e)),
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
    ) -> Result<Status, ErrorKind> {
        let default_chain: RequestChain = Chain::new(vec![
            Box::<Quirks>::default(),
            Box::new(credentials),
            Box::new(self.clone()),
        ]);

        match self.check_website_inner(uri, &default_chain).await {
            Status::Ok(code) if self.require_https && uri.scheme() == "http" => {
                if self
                    .check_website_inner(&uri.to_https()?, &default_chain)
                    .await
                    .is_success()
                {
                    Ok(Status::Error(ErrorKind::InsecureURL(uri.to_https()?)))
                } else {
                    Ok(Status::Ok(code))
                }
            }
            s => Ok(s),
        }
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
        let request = self
            .reqwest_client
            .request(self.method.clone(), uri.as_str())
            .build();

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
