#![cfg(test)]

use http::StatusCode;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

// TODO: used in cli tests (as duplicate)
#[allow(unused)]
pub(crate) async fn get_mock_server<S>(response_code: S) -> MockServer
where
    S: Into<StatusCode>,
{
    get_mock_server_with_content(response_code, None).await
}

pub(crate) async fn get_mock_server_with_content<S>(
    response_code: S,
    content: Option<&str>,
) -> MockServer
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
