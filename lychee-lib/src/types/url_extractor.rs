use super::{FileType, InputContent, InputSource};
use crate::chain::Chain;
use crate::utils::request;
use crate::{BasicAuthExtractor, ChainResult, ErrorKind, Handler, Result, Uri};
use async_trait::async_trait;
use http::HeaderMap;
use reqwest::{Request, Url};

#[derive(Debug, Default, Clone)]
pub struct UrlExtractor {
    pub basic_auth_extractor: Option<BasicAuthExtractor>,
    pub headers: HeaderMap,
    pub client: reqwest::Client,
}

type RequestChain = Chain<reqwest::Request, String>;

impl UrlExtractor {
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
            ChainResult::Next(_) => todo!(),
            ChainResult::Done(r) => r,
        };

        let input_content = InputContent {
            source: InputSource::RemoteUrl(Box::new(url.clone())),
            file_type,
            content,
        };

        Ok(input_content)
    }
}

#[async_trait]
impl Handler<Request, String> for UrlExtractor {
    async fn handle(&mut self, mut input: Request) -> ChainResult<Request, String> {
        *input.headers_mut() = self.headers.clone();

        let result = self
            .client
            .execute(input)
            .await
            .map_err(ErrorKind::NetworkRequest)
            .expect("todo") // todo
            .text()
            .await
            .map_err(ErrorKind::ReadResponseBody)
            .expect("todo"); // todo

        ChainResult::Done(result)
    }
}
