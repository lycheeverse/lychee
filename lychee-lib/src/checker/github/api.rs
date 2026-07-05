use std::{collections::HashSet, sync::Arc};

use http::{Method, StatusCode, header};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use url::Url;

use crate::{Result, Status, Uri, ratelimit::HostPool};

const GITHUB_API_BASE_URL: &str = "https://api.github.com";
const GITHUB_API_ACCEPT: &str = "application/vnd.github+json";

#[derive(Debug)]
pub(crate) struct RepoStatus {
    pub(crate) status: Status,
    pub(crate) private: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RepoResponse {
    private: Option<bool>,
}

/// Minimal GitHub REST API wrapper used by specialized GitHub checks.
#[derive(Debug, Clone)]
pub(crate) struct GitHubApi {
    host_pool: Arc<HostPool>,
    token: Option<SecretString>,
    base_url: Url,
    accepted: HashSet<StatusCode>,
}

impl GitHubApi {
    #[must_use]
    pub(crate) fn new(
        host_pool: Arc<HostPool>,
        token: Option<SecretString>,
        accepted: HashSet<StatusCode>,
    ) -> Self {
        Self {
            host_pool,
            token,
            base_url: Url::parse(GITHUB_API_BASE_URL).expect("GitHub API base URL is valid"),
            accepted,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) const fn with_base_url(
        host_pool: Arc<HostPool>,
        token: Option<SecretString>,
        base_url: Url,
        accepted: HashSet<StatusCode>,
    ) -> Self {
        Self {
            host_pool,
            token,
            base_url,
            accepted,
        }
    }

    pub(crate) async fn check_repo(&self, owner: &str, repo: &str) -> RepoStatus {
        match self
            .get_repo_status(&format!("/repos/{owner}/{repo}"))
            .await
        {
            Ok(status) => status,
            Err(error) => RepoStatus {
                status: error.into(),
                private: None,
            },
        }
    }

    pub(crate) async fn check_repo_readme(
        &self,
        owner: &str,
        repo: &str,
        ref_: Option<&str>,
    ) -> Status {
        match self
            .get_status(
                &format!("/repos/{owner}/{repo}/readme"),
                ref_.map(|r| ("ref", r)),
            )
            .await
        {
            Ok(status) => status,
            Err(error) => error.into(),
        }
    }

    async fn get_status(&self, path: &str, query: Option<(&str, &str)>) -> Result<Status> {
        let request = self.build_request(path, query)?;
        let response = self.host_pool.execute_request(request, false).await?;
        Ok(Status::new(&response, &self.accepted))
    }

    async fn get_repo_status(&self, path: &str) -> Result<RepoStatus> {
        let request = self.build_request(path, None)?;
        let response = self.host_pool.execute_request(request, true).await?;
        let status = Status::new(&response, &self.accepted);
        let private = response
            .text
            .filter(|_| status.is_success())
            .and_then(|text| serde_json::from_str::<RepoResponse>(&text).ok())
            .and_then(|repo| repo.private);

        Ok(RepoStatus { status, private })
    }

    fn build_request(&self, path: &str, query: Option<(&str, &str)>) -> Result<reqwest::Request> {
        let mut url = self.base_url.clone();
        url.set_path(path);
        if let Some((key, value)) = query {
            url.query_pairs_mut().append_pair(key, value);
        }

        let mut request = self.host_pool.build_request(Method::GET, &Uri::from(url))?;
        request.headers_mut().insert(
            header::ACCEPT,
            header::HeaderValue::from_static(GITHUB_API_ACCEPT),
        );

        if let Some(token) = &self.token {
            let value =
                header::HeaderValue::from_str(&format!("Bearer {}", token.expose_secret()))?;
            request.headers_mut().insert(header::AUTHORIZATION, value);
        }

        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use secrecy::SecretString;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path, query_param},
    };

    use crate::{
        checker::github::api::GitHubApi,
        ratelimit::{HostConfigs, HostPool, RateLimitConfig},
        types::DEFAULT_ACCEPTED_STATUS_CODES,
    };

    fn api(server: &MockServer, token: Option<SecretString>) -> GitHubApi {
        let host_pool = HostPool::new(
            RateLimitConfig::default(),
            HostConfigs::default(),
            reqwest::Client::new(),
            HashMap::new(),
        );

        GitHubApi::with_base_url(
            Arc::new(host_pool),
            token,
            server.uri().parse().unwrap(),
            DEFAULT_ACCEPTED_STATUS_CODES.clone(),
        )
    }

    #[tokio::test]
    async fn checks_repository_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/lycheeverse/lychee"))
            .and(header("accept", "application/vnd.github+json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "private": false,
            })))
            .mount(&server)
            .await;

        let status = api(&server, None).check_repo("lycheeverse", "lychee").await;

        assert!(
            status.status.is_success(),
            "expected success, got {:?}",
            status.status
        );
        assert_eq!(status.private, Some(false));
    }

    #[tokio::test]
    async fn checks_repository_readme_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/lycheeverse/lychee/readme"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let status = api(&server, None)
            .check_repo_readme("lycheeverse", "lychee", None)
            .await;

        assert!(status.is_success(), "expected success, got {status:?}");
    }

    #[tokio::test]
    async fn passes_tree_ref_as_query_parameter() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/lycheeverse/lychee/readme"))
            .and(query_param("ref", "main"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let status = api(&server, None)
            .check_repo_readme("lycheeverse", "lychee", Some("main"))
            .await;

        assert!(status.is_success(), "expected success, got {status:?}");
    }

    #[tokio::test]
    async fn parses_private_repository_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/lycheeverse/lychee"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "private": true,
            })))
            .mount(&server)
            .await;

        let status = api(&server, None).check_repo("lycheeverse", "lychee").await;

        assert!(
            status.status.is_success(),
            "expected success, got {:?}",
            status.status
        );
        assert_eq!(status.private, Some(true));
    }

    #[tokio::test]
    async fn sends_authorization_header_when_token_is_configured() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/lycheeverse/lychee"))
            .and(header("authorization", "Bearer secret-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "private": false,
            })))
            .mount(&server)
            .await;

        let status = api(&server, Some(SecretString::from("secret-token")))
            .check_repo("lycheeverse", "lychee")
            .await;

        assert!(
            status.status.is_success(),
            "expected success, got {:?}",
            status.status
        );
    }
}
