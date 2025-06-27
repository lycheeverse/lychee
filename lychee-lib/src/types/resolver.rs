use super::{FileType, InputContent, InputSource};
use crate::chain::Chain;
use crate::utils::request;
use crate::{BasicAuthExtractor, ChainResult, ErrorKind, Handler, Result, Uri};
use async_trait::async_trait;
use http::HeaderMap;
use reqwest::{Client, Request, Url};

/// Structure to fetch remote content.
#[derive(Debug, Default, Clone)]
pub struct UrlContentResolver {
    pub basic_auth_extractor: Option<BasicAuthExtractor>,
    pub headers: HeaderMap,
    pub client: reqwest::Client,
}

type RequestChain = Chain<reqwest::Request, Result<String>>;

impl UrlContentResolver {
    /// Fetch remote content by URL.
    /// This method is not intended to check if a URL is functional but
    /// to get a URL's content and process the content.
    pub async fn url_contents(&self, url: Url) -> Result<InputContent> {
        // Assume HTML for default paths
        let file_type = if url.path().is_empty() || url.path() == "/" {
            FileType::Html
        } else {
            FileType::from(url.as_str())
        };

        let credentials = request::extract_credentials(
            self.basic_auth_extractor.as_ref(),
            &Uri { url: url.clone() },
        );

        let chain: RequestChain = Chain::new(vec![Box::new(credentials), Box::new(self.clone())]);

        let request = self
            .client
            .request(reqwest::Method::GET, url.clone())
            .build()
            .map_err(ErrorKind::BuildRequestClient)?;

        let content = match chain.traverse(request).await {
            ChainResult::Next(_) => unreachable!(
                "ChainResult::Done is unconditionally returned from the last chain element"
            ),
            ChainResult::Done(r) => r,
        }?;

        let input_content = InputContent {
            source: InputSource::RemoteUrl(Box::new(url.clone())),
            file_type,
            content,
        };

        Ok(input_content)
    }
}

#[async_trait]
impl Handler<Request, Result<String>> for UrlContentResolver {
    async fn handle(&mut self, mut request: Request) -> ChainResult<Request, Result<String>> {
        request.headers_mut().extend(self.headers.clone());
        ChainResult::Done(execute_request(&self.client, request).await)
    }
}

async fn execute_request(client: &Client, request: Request) -> Result<String> {
    client
        .execute(request)
        .await
        .map_err(ErrorKind::NetworkRequest)?
        .text()
        .await
        .map_err(ErrorKind::ReadResponseBody)
}
