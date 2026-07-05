//! GitHub-specific URL checking.

use std::{collections::HashSet, sync::Arc};

use http::StatusCode;
use secrecy::SecretString;

use crate::{ErrorKind, FragmentCheckerOptions, Status, Uri, ratelimit::HostPool};

use self::{api::GitHubApi, url::GitHubUrl};

pub(crate) mod api;
pub(crate) mod request;
pub(crate) mod url;

/// Provider-specific checker for GitHub URLs which need API semantics.
#[derive(Debug, Clone)]
pub(crate) struct GitHubChecker {
    api: GitHubApi,
}

impl GitHubChecker {
    #[must_use]
    pub(crate) fn new(
        host_pool: Arc<HostPool>,
        token: Option<SecretString>,
        accepted: HashSet<StatusCode>,
    ) -> Self {
        Self {
            api: GitHubApi::new(host_pool, token, accepted),
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) const fn with_api(api: GitHubApi) -> Self {
        Self { api }
    }

    /// Check GitHub URL shapes that should use provider-specific semantics
    /// before the generic website checker runs.
    pub(crate) async fn check_before_website(
        &self,
        uri: &Uri,
        fragment_options: FragmentCheckerOptions,
    ) -> Option<Status> {
        if !fragment_options.check_anchor_fragments {
            return None;
        }

        let GitHubUrl::RepoReadme { owner, repo, ref_ } = GitHubUrl::parse(uri)? else {
            return None;
        };

        Some(
            self.api
                .check_repo_readme(&owner, &repo, ref_.as_deref())
                .await,
        )
    }

    /// Fall back to the GitHub API after a normal website request failed.
    ///
    /// This keeps private repository checks working without leaving GitHub API
    /// details in the generic website checker.
    pub(crate) async fn check_after_website_failure(&self, status: Status, uri: &Uri) -> Status {
        if status.is_success() {
            return status;
        }

        let Some(url) = GitHubUrl::parse(uri) else {
            return status;
        };

        if matches!(url, GitHubUrl::Repo { .. }) && uri.url.fragment().is_some() {
            return status;
        }

        let Some((owner, repo)) = url.repo_parts() else {
            return status;
        };

        let repo_status = self.api.check_repo(owner, repo).await;
        if !repo_status.status.is_success() {
            return status;
        }

        match url {
            GitHubUrl::Repo { .. } | GitHubUrl::RepoReadme { .. } => repo_status.status,
            GitHubUrl::RepoPath { owner, repo, path } => {
                if repo_status.private == Some(true) {
                    return repo_status.status;
                }
                Status::Error(ErrorKind::InvalidGithubUrl(format!(
                    "{owner}/{repo}/{path}"
                )))
            }
            GitHubUrl::BlobMarkdownFragment { owner, repo, path } => {
                if repo_status.private == Some(true) {
                    return repo_status.status;
                }
                Status::Error(ErrorKind::InvalidGithubUrl(format!(
                    "{owner}/{repo}/blob/{path}"
                )))
            }
            GitHubUrl::BlobLineFragment => status,
        }
    }
}
