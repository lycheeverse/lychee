use std::collections::HashMap;

use http::StatusCode;
use reqwest::Url;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::Uri;

#[allow(unused)]
pub async fn get_mock_server<S>(response_code: S) -> MockServer
where
    S: Into<StatusCode>,
{
    get_mock_server_with_content(response_code, None).await
}

pub async fn get_mock_server_with_content<S>(response_code: S, content: Option<&str>) -> MockServer
where
    S: Into<StatusCode>,
{
    let mock_server = MockServer::start().await;
    let template = ResponseTemplate::new(response_code.into());

    let template = if let Some(s) = content {
        template.set_body_string(s)
    } else {
        template
    };

    Mock::given(path("/"))
        .respond_with(template)
        .mount(&mock_server)
        .await;

    mock_server
}

pub async fn get_mock_server_map<S>(pages: HashMap<&str, (S, Option<&str>)>) -> MockServer
where
    S: Into<StatusCode>,
{
    let mock_server = MockServer::start().await;

    for (route, (response_code, content)) in pages {
        let template = ResponseTemplate::new(response_code.into());

        let template = if let Some(s) = content {
            template.set_body_string(s)
        } else {
            template
        };

        Mock::given(path(route))
            .respond_with(template)
            .mount(&mock_server)
            .await;
    }

    mock_server
}

/// Helper method to convert a string into a URI
/// Note: This panics on error, so it should only be used for testing
pub fn website(url: &str) -> Uri {
    Uri::Website(Url::parse(url).expect("Expected valid Website URI"))
}
