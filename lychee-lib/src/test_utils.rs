use std::convert::TryFrom;

use reqwest::Url;

use crate::{ClientBuilder, ErrorKind, Request, Uri};

#[macro_export]
macro_rules! mock_server {
    ($status:expr $(, $func:tt ($($arg:expr),*))*) => {{
        let mock_server = wiremock::MockServer::start().await;
        let template = wiremock::ResponseTemplate::new(http::StatusCode::from($status));
        let template = template$(.$func($($arg),*))*;
        wiremock::Mock::given(wiremock::matchers::method("GET")).respond_with(template).mount(&mock_server).await;
        mock_server
    }};
}

pub(crate) async fn get_mock_client_response<T, E>(request: T) -> crate::Response
where
    Request: TryFrom<T, Error = E>,
    ErrorKind: From<E>,
{
    ClientBuilder::default()
        .build()
        .unwrap()
        .check(request)
        .await
        .unwrap()
}

/// Helper method to convert a string into a URI
/// Note: This panics on error, so it should only be used for testing
pub(crate) fn website(url: &str) -> Uri {
    Uri::from(Url::parse(url).expect("Expected valid Website URI"))
}

pub(crate) fn mail(address: &str) -> Uri {
    if address.starts_with("mailto:") {
        Url::parse(address)
    } else {
        Url::parse(&(String::from("mailto:") + address))
    }
    .expect("Expected valid Mail Address")
    .into()
}
