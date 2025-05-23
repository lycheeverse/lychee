use std::{convert::TryFrom, fs, path::Path};

use reqwest::Url;

use crate::{ClientBuilder, ErrorKind, Request, Uri};

#[macro_export]
/// Creates a mock web server, which responds with a predefined status when
/// handling a matching request
macro_rules! mock_server {
    ($status:expr $(, $func:tt ($($arg:expr),*))*) => {{
        let mock_server = wiremock::MockServer::start().await;
        let response_template = wiremock::ResponseTemplate::new(http::StatusCode::from($status));
        let template = response_template$(.$func($($arg),*))*;
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
        .client()
        .unwrap()
        .check(request)
        .await
        .unwrap()
}

/// Helper method to convert a string into a URI
///
/// # Panic
///
/// This panics on error, so it should only be used for testing
pub(crate) fn website(url: &str) -> Uri {
    Uri::from(Url::parse(url).expect("Expected valid Website URI"))
}

/// Helper method to convert a `std::path::Path `into a URI with the `file` scheme
///
/// # Panic
///
/// This panics if the given path is not absolute, so it should only be used for
/// testing
pub(crate) fn path<P: AsRef<Path>>(path: P) -> Uri {
    Uri::from(Url::from_file_path(path.as_ref()).expect("Expected valid File URI"))
}

/// Creates a mail URI from a string
pub(crate) fn mail(address: &str) -> Uri {
    if address.starts_with("mailto:") {
        Url::parse(address)
    } else {
        Url::parse(&(String::from("mailto:") + address))
    }
    .expect("Expected valid Mail Address")
    .into()
}

/// Loads a fixture from the `fixtures` directory
pub(crate) fn load_fixture(filename: &str) -> String {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("fixtures")
        .join(filename);
    fs::read_to_string(fixture_path).unwrap()
}
