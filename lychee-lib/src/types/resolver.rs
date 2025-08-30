use super::{FileType, InputContent, InputSource};
use crate::utils::request;
use crate::{BasicAuthExtractor, ErrorKind, Result, Uri};
use http::HeaderMap;
use reqwest::{Client, Request, Url};
use crate::types::input::source::ResolvedInputSource;

/// Structure to fetch remote content.
#[derive(Debug, Default, Clone)]
pub struct UrlContentResolver {
    pub basic_auth_extractor: Option<BasicAuthExtractor>,
    pub headers: HeaderMap,
    pub client: reqwest::Client,
}

impl UrlContentResolver {
    /// Fetch remote content by URL.
    ///
    /// This method is not intended to check if a URL is functional but
    /// to get a URL's content and process the content.
    pub async fn url_contents(&self, url: Url) -> Result<InputContent> {
        // Assume HTML for default paths
        let file_type = match url.path() {
            path if path.is_empty() || path == "/" => FileType::Html,
            _ => FileType::from(url.as_str()),
        };

        let credentials = request::extract_credentials(
            self.basic_auth_extractor.as_ref(),
            &Uri { url: url.clone() },
        );

        let request = self.build_request(&url, credentials)?;
        let content = get_request_body_text(&self.client, request).await?;

        let input_content = InputContent {
            source: ResolvedInputSource::RemoteUrl(Box::new(url.clone())),
            file_type,
            content,
        };

        Ok(input_content)
    }

    fn build_request(
        &self,
        url: &Url,
        credentials: Option<super::BasicAuthCredentials>,
    ) -> Result<Request> {
        let mut request = self
            .client
            .request(reqwest::Method::GET, url.clone())
            .build()
            .map_err(ErrorKind::BuildRequestClient)?;

        request.headers_mut().extend(self.headers.clone());
        if let Some(credentials) = credentials {
            credentials.append_to_request(&mut request);
        }

        Ok(request)
    }
}

async fn get_request_body_text(client: &Client, request: Request) -> Result<String> {
    client
        .execute(request)
        .await
        .map_err(ErrorKind::NetworkRequest)?
        .text()
        .await
        .map_err(ErrorKind::ReadResponseBody)
}
