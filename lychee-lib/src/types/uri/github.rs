use std::hash::{Hash, Hasher};
use std::{collections::HashSet, sync::LazyLock};

use thiserror::Error;

use crate::Uri;

/// Errors that can occur while resolving and checking GitHub URLs through the
/// GitHub API.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum GithubError {
    /// The GitHub API could not be called because of a missing GitHub token.
    #[error("GitHub token required")]
    MissingToken,

    /// The GitHub client required for making requests cannot be created.
    #[error("Failed to create GitHub client")]
    BuildClient(#[source] Box<octocrab::Error>),

    /// Network error while using the GitHub API.
    #[error("Network error while using GitHub client")]
    Request(#[source] Box<octocrab::Error>),

    /// The URL could not be interpreted as a valid GitHub repository URL.
    #[error("GitHub URL is invalid: {0}")]
    InvalidUrl(String),
}

impl GithubError {
    /// Return more details about this error, including remediation hints.
    #[must_use]
    pub fn details(&self) -> String {
        match self {
            GithubError::Request(e) => {
                let detail = if let octocrab::Error::GitHub { source, .. } = &**e {
                    source.message.clone()
                } else {
                    e.to_string()
                };
                format!("{self}: {detail}")
            }
            GithubError::BuildClient(error) => {
                format!("{self}: {error}. Check token and network connectivity")
            }
            GithubError::InvalidUrl(_) => format!("{self}. Check URL syntax"),
            GithubError::MissingToken => {
                format!("{self}. Use --github-token flag or GITHUB_TOKEN environment variable")
            }
        }
    }
}

impl PartialEq for GithubError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Request(a), Self::Request(b)) | (Self::BuildClient(a), Self::BuildClient(b)) => {
                a.to_string() == b.to_string()
            }
            (Self::InvalidUrl(a), Self::InvalidUrl(b)) => a == b,
            (Self::MissingToken, Self::MissingToken) => true,
            _ => false,
        }
    }
}

impl Eq for GithubError {}

impl Hash for GithubError {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Request(e) | Self::BuildClient(e) => e.to_string().hash(state),
            Self::InvalidUrl(s) => s.hash(state),
            Self::MissingToken => std::mem::discriminant(self).hash(state),
        }
    }
}

static GITHUB_API_EXCLUDED_ENDPOINTS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from_iter([
        "about",
        "collections",
        "events",
        "explore",
        "features",
        "issues",
        "marketplace",
        "new",
        "notifications",
        "pricing",
        "pulls",
        "sponsors",
        "topics",
        "watching",
    ])
});

/// Uri path segments extracted from a GitHub URL
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct GithubUri {
    /// Organization name
    pub owner: String,
    /// Repository name
    pub repo: String,
    /// e.g. `issues` in `/org/repo/issues`
    pub endpoint: Option<String>,
}

impl GithubUri {
    /// Create a new GitHub URI without an endpoint
    #[cfg(test)]
    fn new<T: Into<String>>(owner: T, repo: T) -> Self {
        GithubUri {
            owner: owner.into(),
            repo: repo.into(),
            endpoint: None,
        }
    }

    #[cfg(test)]
    fn with_endpoint<T: Into<String>>(owner: T, repo: T, endpoint: T) -> Self {
        GithubUri {
            owner: owner.into(),
            repo: repo.into(),
            endpoint: Some(endpoint.into()),
        }
    }

    // TODO: Support GitLab etc.
    fn gh_org_and_repo(uri: &Uri) -> Result<GithubUri, GithubError> {
        fn remove_suffix<'a>(input: &'a str, suffix: &str) -> &'a str {
            if let Some(stripped) = input.strip_suffix(suffix) {
                return stripped;
            }
            input
        }

        debug_assert!(!uri.is_mail(), "Should only be called on a Website type!");

        let Some(domain) = uri.domain() else {
            return Err(GithubError::InvalidUrl(uri.to_string()));
        };

        if !matches!(
            domain,
            "github.com" | "www.github.com" | "raw.githubusercontent.com"
        ) {
            return Err(GithubError::InvalidUrl(uri.to_string()));
        }

        let parts: Vec<_> = match uri.path_segments() {
            Some(parts) => parts.collect(),
            None => return Err(GithubError::InvalidUrl(uri.to_string())),
        };

        if parts.len() < 2 {
            // Not a valid org/repo pair.
            // Note: We don't check for exactly 2 here, because the GitHub
            // API doesn't handle checking individual files inside repos or
            // paths like <github.com/org/repo/issues>, so we are more
            // permissive and only check for repo existence. This is the
            // only way to get a basic check for private repos. Public repos
            // are not affected and should work with a normal check.
            return Err(GithubError::InvalidUrl(uri.to_string()));
        }

        let owner = parts[0];
        if GITHUB_API_EXCLUDED_ENDPOINTS.contains(owner) {
            return Err(GithubError::InvalidUrl(uri.to_string()));
        }

        let repo = parts[1];
        // If the URL ends with `.git`, assume this is an SSH URL and strip
        // the suffix. See https://github.com/lycheeverse/lychee/issues/384
        let repo = remove_suffix(repo, ".git");

        let endpoint = if parts.len() > 2 && !parts[2].is_empty() {
            Some(parts[2..].join("/"))
        } else {
            None
        };

        Ok(GithubUri {
            owner: owner.to_string(),
            repo: repo.to_string(),
            endpoint,
        })
    }
}

impl TryFrom<Uri> for GithubUri {
    type Error = GithubError;

    fn try_from(uri: Uri) -> Result<Self, GithubError> {
        GithubUri::gh_org_and_repo(&uri)
    }
}

impl TryFrom<&Uri> for GithubUri {
    type Error = GithubError;

    fn try_from(uri: &Uri) -> Result<Self, GithubError> {
        GithubUri::gh_org_and_repo(uri)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use test_utils::website;

    #[test]
    fn test_github() {
        assert_eq!(
            GithubUri::try_from(website!("http://github.com/lycheeverse/lychee")).unwrap(),
            GithubUri::new("lycheeverse", "lychee")
        );

        assert_eq!(
            GithubUri::try_from(website!("http://www.github.com/lycheeverse/lychee")).unwrap(),
            GithubUri::new("lycheeverse", "lychee")
        );

        assert_eq!(
            GithubUri::try_from(website!("https://github.com/lycheeverse/lychee")).unwrap(),
            GithubUri::new("lycheeverse", "lychee")
        );

        assert_eq!(
            GithubUri::try_from(website!("https://github.com/lycheeverse/lychee/")).unwrap(),
            GithubUri::new("lycheeverse", "lychee")
        );

        assert_eq!(
            GithubUri::try_from(website!("https://github.com/lycheeverse/lychee/foo/bar")).unwrap(),
            GithubUri::with_endpoint("lycheeverse", "lychee", "foo/bar")
        );

        assert_eq!(
            GithubUri::try_from(website!(
                "https://github.com/Microsoft/python-language-server.git"
            ))
            .unwrap(),
            GithubUri::new("Microsoft", "python-language-server")
        );

        assert_eq!(
            GithubUri::try_from(website!(
                "https://github.com/lycheeverse/lychee/blob/master/NON_EXISTENT_FILE.md"
            ))
            .unwrap(),
            GithubUri::with_endpoint("lycheeverse", "lychee", "blob/master/NON_EXISTENT_FILE.md")
        );
    }

    #[test]
    fn test_github_false_positives() {
        assert!(
            GithubUri::try_from(website!("https://github.com/sponsors/analysis-tools-dev "))
                .is_err()
        );

        assert!(
            GithubUri::try_from(website!(
                "https://github.com/marketplace/actions/lychee-broken-link-checker"
            ))
            .is_err()
        );

        assert!(GithubUri::try_from(website!("https://github.com/features/actions")).is_err());

        assert!(
            GithubUri::try_from(website!(
                "https://pkg.go.dev/github.com/Debian/pkg-go-tools/cmd/pgt-gopath"
            ))
            .is_err()
        );
    }
}
