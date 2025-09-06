//! `test-utils` is used for testing in both `lychee-lib` and `lychee-bin`.
//! This crate does not depend on any other crates.
//! Macros are used instead, so that the importer is responsible for providing the dependencies.

#[macro_export]
/// Create a mock web server, which responds with a predefined status when
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

#[macro_export]
/// Set up a mock server which has two routes: `/ok` and `/redirect`.
/// Calling `/redirect` returns a HTTP Location header redirecting to `/ok`
macro_rules! redirecting_mock_server {
    ($f:expr) => {{
        use std::str::FromStr;
        use url::Url;

        async {
            let mock_server = wiremock::MockServer::start().await;
            let ok_url = Url::from_str(&format!("{}/ok", mock_server.uri())).unwrap();
            let redirect_url = Url::from_str(&format!("{}/redirect", mock_server.uri())).unwrap();

            // Set up redirect
            let redirect = wiremock::ResponseTemplate::new(StatusCode::PERMANENT_REDIRECT)
                .insert_header("Location", ok_url.as_str());
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path("/redirect"))
                .respond_with(redirect)
                .expect(1) // expect the redirect to be followed and called once
                .mount(&mock_server)
                .await;

            let ok = wiremock::ResponseTemplate::new(StatusCode::OK);
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path("/ok"))
                .respond_with(ok)
                .expect(1) // expect the redirect to be followed and called once
                .mount(&mock_server)
                .await;

            $f(redirect_url, ok_url).await;
        }
    }};
}

#[macro_export]
macro_rules! get_mock_client_response {
    ($request:expr $(,)?) => {
        async {
            ClientBuilder::default()
                .client()
                .unwrap()
                .check($request)
                .await
                .unwrap()
        }
    };
}

#[macro_export]
/// Helper method to convert a `std::path::Path `into a URI with the `file` scheme
///
/// # Panic
///
/// This panics if the given path is not absolute, so it should only be used for
/// testing
macro_rules! path {
    ($path:expr) => {
        Uri::from(Url::from_file_path(String::from($path)).expect("Expected valid File URI"))
    };
}

#[macro_export]
/// Helper method to convert a string into a URI
///
/// # Panic
///
/// This panics on error, so it should only be used for testing
macro_rules! website {
    ($url:expr) => {{
        use url::Url;
        Uri::from(Url::parse($url).expect("Expected valid Website URL"))
    }};
}

#[macro_export]
/// Creates a mail URI from a string
macro_rules! mail {
    ($address:expr) => {{
        use url::Url;

        if $address.starts_with("mailto:") {
            Url::parse($address)
        } else {
            Url::parse(&(String::from("mailto:") + $address))
        }
        .expect("Expected valid Mail Address")
        .into()
    }};
}

#[macro_export]
/// Returns the path to the `fixtures` directory.
///
/// # Panic
///
/// Panics if the fixtures directory could not be determined.
macro_rules! fixtures_path {
    () => {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("fixtures")
    };
}

#[macro_export]
/// Loads a fixture from the `fixtures` directory
macro_rules! load_fixture {
    ($filename:expr) => {{
        let path = fixtures_path!().join($filename);
        std::fs::read_to_string(path).unwrap()
    }};
}

#[macro_export]
/// Constructs a [`Uri`] from a given subpath within the `fixtures` directory.
///
/// The specified subpath may contain a fragment reference by ending with `#something`.
/// The subpath should not begin with a slash, otherwise it will be treated as an
/// absolute path.
macro_rules! fixture_uri {
    ($subpath:expr) => {{
        use url::Url;
        let fixture_url =
            Url::from_directory_path(fixtures_path!()).expect("fixture path should be a valid URL");

        // joining subpath onto a Url allows the subpath to contain a fragment
        fixture_url
            .join($subpath)
            .expect("expected subpath to form a valid URL")
    }};
}
