use async_trait::async_trait;
use log::debug;
use reqwest::{Request, Url};

use crate::{
    Status,
    chain::{ChainResult, Handler},
};

use super::url::GitHubUrl;

/// Request-chain handler for syntactic GitHub URL rewrites.
///
/// These rewrites do not call the GitHub API. API-based GitHub behavior belongs
/// in [`super::GitHubChecker`].
#[derive(Debug, Clone, Default)]
pub(crate) struct GitHubRequestRewriter;

impl GitHubRequestRewriter {
    #[must_use]
    pub(crate) fn apply(mut request: Request) -> Request {
        match GitHubUrl::parse_url(request.url()) {
            Some(GitHubUrl::BlobLineFragment) => {
                debug!("Removed GitHub line-number fragment from {}", request.url());
                request.url_mut().set_fragment(None);
            }
            Some(GitHubUrl::BlobMarkdownFragment { owner, repo, path }) => {
                debug!(
                    "Rewriting GitHub Markdown blob to raw content: {}",
                    request.url()
                );
                let fragment = request.url().fragment().map(ToOwned::to_owned);
                let mut raw_url = Url::parse(&format!(
                    "https://raw.githubusercontent.com/{owner}/{repo}/{path}"
                ))
                .expect("raw GitHub URL should be valid");
                raw_url.set_fragment(fragment.as_deref());
                *request.url_mut() = raw_url;
            }
            _ => {}
        }

        request
    }
}

#[async_trait]
impl Handler<Request, Status> for GitHubRequestRewriter {
    async fn handle(&mut self, input: Request) -> ChainResult<Request, Status> {
        ChainResult::Next(Self::apply(input))
    }
}

#[cfg(test)]
mod tests {
    use http::Method;
    use reqwest::{Request, Url};
    use rstest::rstest;

    use super::GitHubRequestRewriter;

    fn assert_rewrite(origin: &str, expected: &str) {
        let request = Request::new(Method::GET, Url::parse(origin).unwrap());
        let modified = GitHubRequestRewriter::apply(request);

        assert_eq!(modified.url(), &Url::parse(expected).unwrap());
        assert_eq!(modified.method(), Method::GET);
    }

    #[rstest]
    #[case::markdown(
        "https://github.com/moby/docker-image-spec/blob/main/spec.md#terminology",
        "https://raw.githubusercontent.com/moby/docker-image-spec/main/spec.md#terminology"
    )]
    #[case::markdown_extension(
        "https://github.com/moby/docker-image-spec/blob/main/spec.markdown#terminology",
        "https://raw.githubusercontent.com/moby/docker-image-spec/main/spec.markdown#terminology"
    )]
    #[case::tag(
        "https://github.com/lycheeverse/lychee/blob/v0.15.0/README.md#features",
        "https://raw.githubusercontent.com/lycheeverse/lychee/v0.15.0/README.md#features"
    )]
    fn rewrites_github_markdown_blob_fragments_to_raw_content(
        #[case] origin: &str,
        #[case] expected: &str,
    ) {
        assert_rewrite(origin, expected);
    }

    #[rstest]
    #[case::single_line(
        "https://github.com/lycheeverse/lychee/blob/master/README.md#L10",
        "https://github.com/lycheeverse/lychee/blob/master/README.md"
    )]
    #[case::range(
        "https://github.com/lycheeverse/lychee/blob/master/src/main.rs#L10-L20",
        "https://github.com/lycheeverse/lychee/blob/master/src/main.rs"
    )]
    #[case::shorthand_range(
        "https://github.com/lycheeverse/lychee/blob/master/src/lib.rs#L5-15",
        "https://github.com/lycheeverse/lychee/blob/master/src/lib.rs"
    )]
    fn removes_github_line_number_fragments(#[case] origin: &str, #[case] expected: &str) {
        assert_rewrite(origin, expected);
    }

    #[rstest]
    #[case::markdown(
        "https://www.github.com/owner/repo/blob/main/README.md#readme",
        "https://raw.githubusercontent.com/owner/repo/main/README.md#readme"
    )]
    #[case::line_number(
        "https://www.github.com/owner/repo/blob/main/README.md#L1",
        "https://www.github.com/owner/repo/blob/main/README.md"
    )]
    fn rewrites_www_github_ui_urls(#[case] origin: &str, #[case] expected: &str) {
        assert_rewrite(origin, expected);
    }

    #[rstest]
    fn leaves_raw_urls_untouched(
        #[values(
            "",
            "main/README.md",
            "blob/README.md",
            "blob/main/README.md",
            "tree/main"
        )]
        path: &str,
        #[values("", "#readme", "#L1", "#terminology")] fragment: &str,
    ) {
        let url = format!("https://raw.githubusercontent.com/owner/repo/{path}{fragment}");
        assert_rewrite(&url, &url);
    }

    #[rstest]
    #[case::markdown_without_fragment(
        "https://github.com/moby/docker-image-spec/blob/main/spec.md"
    )]
    #[case::non_github("https://endler.dev")]
    fn leaves_other_urls_untouched(#[case] url: &str) {
        assert_rewrite(url, url);
    }
}
