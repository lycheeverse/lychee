use std::sync::Arc;

use super::{FileType, InputContent, ResolvedInputSource};
use crate::ratelimit::HostPool;
use crate::utils::request;
use crate::{BasicAuthExtractor, Result, Uri};
use http::HeaderMap;
use reqwest::{Request, Url};

/// Structure to fetch remote content.
///
/// Uses the same [`HostPool`] as the link checker, so input URL fetches
/// benefit from the same user-agent, custom headers, TLS settings, and
/// per-host rate limiting as regular link checks.
#[derive(Debug, Clone)]
pub struct UrlContentResolver {
    pub basic_auth_extractor: Option<BasicAuthExtractor>,
    pub headers: HeaderMap,
    pub host_pool: Arc<HostPool>,
}

impl Default for UrlContentResolver {
    fn default() -> Self {
        Self {
            basic_auth_extractor: None,
            headers: HeaderMap::new(),
            host_pool: Arc::new(HostPool::default()),
        }
    }
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

        // Fetch with body since we need to extract links from the content.
        let response = self.host_pool.execute_request(request, true).await?;

        if !response.status.is_success() {
            return Err(crate::ErrorKind::ReadInputUrlStatusCode(response.status));
        }

        // SAFETY: needs_body=true above guarantees text is populated on success.
        let content = response.text.unwrap_or_else(|| {
            unreachable!("execute_request with needs_body=true always returns text")
        });

        Ok(InputContent {
            source: ResolvedInputSource::RemoteUrl(Box::new(url)),
            file_type,
            content,
        })
    }

    fn build_request(
        &self,
        url: &Url,
        credentials: Option<super::BasicAuthCredentials>,
    ) -> Result<Request> {
        let mut request = self
            .host_pool
            .build_request(reqwest::Method::GET, &url.clone().into())?;

        request.headers_mut().extend(self.headers.clone());
        if let Some(credentials) = credentials {
            credentials.append_to_request(&mut request);
        }

        Ok(request)
    }
}
