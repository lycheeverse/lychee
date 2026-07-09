use std::{collections::HashSet, path::Path, sync::LazyLock};

use url::Url;

use crate::Uri;

static GITHUB_RESERVED_OWNERS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
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

/// GitHub URL shapes which lychee treats specially.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GitHubUrl {
    /// Repository root, e.g. `https://github.com/owner/repo`.
    Repo { owner: String, repo: String },
    /// Path inside a repository. This is used by the API fallback after a normal
    /// website request fails.
    RepoPath {
        owner: String,
        repo: String,
        path: String,
    },
    /// Repository README anchor, e.g. `https://github.com/owner/repo#readme`.
    RepoReadme {
        owner: String,
        repo: String,
        ref_: Option<String>,
    },
    /// Markdown blob URL with a non-line fragment, which can be checked via raw
    /// content and lychee's normal markdown fragment checker.
    BlobMarkdownFragment {
        owner: String,
        repo: String,
        path: String,
    },
    /// Blob URL with a GitHub line-number fragment. GitHub renders these in the
    /// UI, but they are not content fragments lychee can validate generically.
    BlobLineFragment,
}

impl GitHubUrl {
    /// Parse a lychee URI into a supported GitHub URL shape.
    #[must_use]
    pub(crate) fn parse(uri: &Uri) -> Option<Self> {
        Self::parse_url(&uri.url)
    }

    /// Parse a URL into a supported GitHub URL shape.
    #[must_use]
    pub(crate) fn parse_url(url: &Url) -> Option<Self> {
        if url.domain()? != "github.com" {
            return None;
        }

        let segments: Vec<_> = url
            .path_segments()?
            .filter(|segment| !segment.is_empty())
            .collect();

        let owner = *segments.first()?;
        if GITHUB_RESERVED_OWNERS.contains(owner) {
            return None;
        }

        match segments.as_slice() {
            [owner, repo] => Some(parse_repo(owner, repo, url.fragment())),
            [owner, repo, "tree", ref_] if url.fragment() == Some("readme") => {
                Some(Self::RepoReadme {
                    owner: (*owner).to_owned(),
                    repo: strip_git_suffix(repo).to_owned(),
                    ref_: Some((*ref_).to_owned()),
                })
            }
            [owner, repo, "blob", rest @ ..] => Some(parse_blob(owner, repo, rest, url.fragment())),
            [owner, repo, rest @ ..] => Some(Self::RepoPath {
                owner: (*owner).to_owned(),
                repo: strip_git_suffix(repo).to_owned(),
                path: rest.join("/"),
            }),
            _ => None,
        }
    }

    #[must_use]
    pub(crate) fn repo_parts(&self) -> Option<(&str, &str)> {
        match self {
            Self::Repo { owner, repo }
            | Self::RepoPath { owner, repo, .. }
            | Self::RepoReadme { owner, repo, .. }
            | Self::BlobMarkdownFragment { owner, repo, .. } => Some((owner, repo)),
            Self::BlobLineFragment => None,
        }
    }
}

fn parse_repo(owner: &str, repo: &str, fragment: Option<&str>) -> GitHubUrl {
    if fragment == Some("readme") {
        GitHubUrl::RepoReadme {
            owner: owner.to_owned(),
            repo: strip_git_suffix(repo).to_owned(),
            ref_: None,
        }
    } else {
        GitHubUrl::Repo {
            owner: owner.to_owned(),
            repo: strip_git_suffix(repo).to_owned(),
        }
    }
}

fn parse_blob(owner: &str, repo: &str, rest: &[&str], fragment: Option<&str>) -> GitHubUrl {
    if is_line_fragment(fragment) {
        return GitHubUrl::BlobLineFragment;
    }

    if fragment.is_none() || !rest.last().is_some_and(|file| is_markdown_file(file)) {
        return GitHubUrl::RepoPath {
            owner: owner.to_owned(),
            repo: strip_git_suffix(repo).to_owned(),
            path: format!("blob/{}", rest.join("/")),
        };
    }

    GitHubUrl::BlobMarkdownFragment {
        owner: owner.to_owned(),
        repo: strip_git_suffix(repo).to_owned(),
        path: rest.join("/"),
    }
}

fn strip_git_suffix(input: &str) -> &str {
    input.strip_suffix(".git").unwrap_or(input)
}

fn is_markdown_file(file: &str) -> bool {
    Path::new(file).extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
    })
}

fn is_line_fragment(fragment: Option<&str>) -> bool {
    let Some(fragment) = fragment else {
        return false;
    };

    let Some(rest) = fragment.strip_prefix('L') else {
        return false;
    };

    let Some((start, end)) = rest.split_once('-') else {
        return rest.parse::<usize>().is_ok();
    };

    start.parse::<usize>().is_ok()
        && end
            .strip_prefix('L')
            .unwrap_or(end)
            .parse::<usize>()
            .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Option<GitHubUrl> {
        GitHubUrl::parse(&Uri::try_from(input).unwrap())
    }

    #[test]
    fn parses_repository() {
        assert_eq!(
            parse("https://github.com/lycheeverse/lychee"),
            Some(GitHubUrl::Repo {
                owner: "lycheeverse".to_owned(),
                repo: "lychee".to_owned(),
            })
        );
    }

    #[test]
    fn strips_git_suffix_from_repository() {
        assert_eq!(
            parse("https://github.com/Microsoft/python-language-server.git"),
            Some(GitHubUrl::Repo {
                owner: "Microsoft".to_owned(),
                repo: "python-language-server".to_owned(),
            })
        );
    }

    #[test]
    fn parses_repository_path() {
        assert_eq!(
            parse("https://github.com/lycheeverse/lychee/blob/master/NON_EXISTENT_FILE.md"),
            Some(GitHubUrl::RepoPath {
                owner: "lycheeverse".to_owned(),
                repo: "lychee".to_owned(),
                path: "blob/master/NON_EXISTENT_FILE.md".to_owned(),
            })
        );
    }

    #[test]
    fn parses_repository_readme_fragment() {
        assert_eq!(
            parse("https://github.com/lycheeverse/lychee#readme"),
            Some(GitHubUrl::RepoReadme {
                owner: "lycheeverse".to_owned(),
                repo: "lychee".to_owned(),
                ref_: None,
            })
        );
    }

    #[test]
    fn parses_repository_tree_readme_fragment() {
        assert_eq!(
            parse("https://github.com/lycheeverse/lychee/tree/main#readme"),
            Some(GitHubUrl::RepoReadme {
                owner: "lycheeverse".to_owned(),
                repo: "lychee".to_owned(),
                ref_: Some("main".to_owned()),
            })
        );
    }

    #[test]
    fn ignores_tree_urls_with_path_or_slash_branch_names_for_now() {
        assert_eq!(
            parse("https://github.com/lycheeverse/lychee/tree/feat/readme-checker#readme"),
            Some(GitHubUrl::RepoPath {
                owner: "lycheeverse".to_owned(),
                repo: "lychee".to_owned(),
                path: "tree/feat/readme-checker".to_owned(),
            })
        );
    }

    #[test]
    fn parses_markdown_blob_fragments() {
        assert_eq!(
            parse("https://github.com/moby/docker-image-spec/blob/main/spec.md#terminology"),
            Some(GitHubUrl::BlobMarkdownFragment {
                owner: "moby".to_owned(),
                repo: "docker-image-spec".to_owned(),
                path: "main/spec.md".to_owned(),
            })
        );
    }

    #[test]
    fn parses_blob_line_fragments() {
        assert_eq!(
            parse("https://github.com/lycheeverse/lychee/blob/master/README.md#L10-L20"),
            Some(GitHubUrl::BlobLineFragment)
        );
        assert_eq!(
            parse("https://github.com/lycheeverse/lychee/blob/master/src/lib.rs#L5-15"),
            Some(GitHubUrl::BlobLineFragment)
        );
    }

    #[test]
    fn ignores_reserved_github_paths() {
        assert_eq!(parse("https://github.com/features/actions"), None);
        assert_eq!(
            parse("https://github.com/sponsors/analysis-tools-dev"),
            None
        );
    }

    #[test]
    fn ignores_non_github_urls() {
        assert_eq!(parse("https://example.com/lycheeverse/lychee#readme"), None);
    }
}
