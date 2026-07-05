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

    use super::GitHubRequestRewriter;

    #[derive(Debug)]
    struct MockRequest(Request);

    impl MockRequest {
        fn new(method: Method, url: Url) -> Self {
            Self(Request::new(method, url))
        }
    }

    impl PartialEq for MockRequest {
        fn eq(&self, other: &Self) -> bool {
            self.0.url() == other.0.url() && self.0.method() == other.0.method()
        }
    }

    #[test]
    fn rewrites_github_markdown_blob_fragments_to_raw_content() {
        let cases = [
            (
                "https://github.com/moby/docker-image-spec/blob/main/spec.md#terminology",
                "https://raw.githubusercontent.com/moby/docker-image-spec/main/spec.md#terminology",
            ),
            (
                "https://github.com/moby/docker-image-spec/blob/main/spec.markdown#terminology",
                "https://raw.githubusercontent.com/moby/docker-image-spec/main/spec.markdown#terminology",
            ),
            (
                "https://github.com/lycheeverse/lychee/blob/v0.15.0/README.md#features",
                "https://raw.githubusercontent.com/lycheeverse/lychee/v0.15.0/README.md#features",
            ),
        ];

        for (origin, expected) in cases {
            let request = Request::new(Method::GET, Url::parse(origin).unwrap());
            let modified = GitHubRequestRewriter::apply(request);

            assert_eq!(
                MockRequest(modified),
                MockRequest::new(Method::GET, Url::parse(expected).unwrap())
            );
        }
    }

    #[test]
    fn leaves_github_markdown_blob_without_fragment_untouched() {
        let url =
            Url::parse("https://github.com/moby/docker-image-spec/blob/main/spec.md").unwrap();
        let request = Request::new(Method::GET, url.clone());
        let modified = GitHubRequestRewriter::apply(request);

        assert_eq!(MockRequest(modified), MockRequest::new(Method::GET, url));
    }

    #[test]
    fn removes_github_line_number_fragments() {
        let cases = [
            (
                "https://github.com/lycheeverse/lychee/blob/master/README.md#L10",
                "https://github.com/lycheeverse/lychee/blob/master/README.md",
            ),
            (
                "https://github.com/lycheeverse/lychee/blob/master/src/main.rs#L10-L20",
                "https://github.com/lycheeverse/lychee/blob/master/src/main.rs",
            ),
            (
                "https://github.com/lycheeverse/lychee/blob/master/src/lib.rs#L5-15",
                "https://github.com/lycheeverse/lychee/blob/master/src/lib.rs",
            ),
        ];

        for (origin, expected) in cases {
            let request = Request::new(Method::GET, Url::parse(origin).unwrap());
            let modified = GitHubRequestRewriter::apply(request);

            assert_eq!(
                MockRequest(modified),
                MockRequest::new(Method::GET, Url::parse(expected).unwrap())
            );
        }
    }

    #[test]
    fn leaves_non_github_urls_untouched() {
        let url = Url::parse("https://endler.dev").unwrap();
        let request = Request::new(Method::GET, url.clone());
        let modified = GitHubRequestRewriter::apply(request);

        assert_eq!(MockRequest(modified), MockRequest::new(Method::GET, url));
    }
}
