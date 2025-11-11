//! `test-utils` is used for testing in both `lychee-lib` and `lychee-bin`.
//! This crate does not depend on `lychee-lib` or `lychee-bin`, else we would get dependency cycles.
//! Macros are used instead, so that the importer is responsible for providing the dependencies.

/// Create a mock web server, which responds with a predefined status when
/// handling a matching request
#[macro_export]
macro_rules! mock_server {
    ($status:expr $(, $func:tt ($($arg:expr),*))*) => {{
        let mock_server = wiremock::MockServer::start().await;
        let response_template = wiremock::ResponseTemplate::new(http::StatusCode::from($status));
        let template = response_template$(.$func($($arg),*))*;
        wiremock::Mock::given(wiremock::matchers::method("GET")).respond_with(template).mount(&mock_server).await;
        mock_server
    }};
}

/// Set up a mock server which has two routes: `/ok` and `/redirect`.
/// Calling `/redirect` returns a HTTP Location header redirecting to `/ok`
#[macro_export]
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

/// Helper method to convert a `std::path::Path `into a URI with the `file` scheme
///
/// # Panic
///
/// This panics if the given path is not absolute, so it should only be used for
/// testing
#[macro_export]
macro_rules! path {
    ($path:expr) => {
        Uri::from(Url::from_file_path(String::from($path)).expect("Expected valid File URI"))
    };
}

/// Helper method to convert a string into a URI
///
/// # Panic
///
/// This panics on error, so it should only be used for testing
#[macro_export]
macro_rules! website {
    ($url:expr) => {{
        use url::Url;
        Uri::from(Url::parse($url).expect("Expected valid Website URL"))
    }};
}

/// Creates a mail URI from a string
#[macro_export]
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

/// Get the root path of the project.
#[macro_export]
macro_rules! root_path {
    () => {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    };
}

/// Get the path to the `fixtures` directory.
#[macro_export]
macro_rules! fixtures_path {
    () => {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("fixtures")
    };
}

/// Loads a fixture from the `fixtures` directory
#[macro_export]
macro_rules! load_fixture {
    ($filename:expr) => {{
        let path = fixtures_path!().join($filename);
        std::fs::read_to_string(path).unwrap()
    }};
}

/// Constructs a `Uri` from a given subpath within the `fixtures` directory.
///
/// The specified subpath may contain a fragment reference by ending with `#something`.
/// The subpath should not begin with a slash, otherwise it will be treated as an
/// absolute path.
#[macro_export]
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

#[macro_export]
macro_rules! load_readme_text {
    () => {{
        let readme_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("README.md");
        std::fs::read_to_string(readme_path).unwrap()
    }};
}

/// Helper function to create a ResponseBody with a given status and URI
#[macro_export]
macro_rules! mock_response_body {
    ($status:expr, $uri:expr $(,)?) => {{
        ResponseBody {
            uri: Uri::try_from($uri).unwrap(),
            status: $status,
        }
    }};
}

/// Gets the "main" binary name (e.g. `lychee`)
#[macro_export]
macro_rules! main_command {
    () => {
        Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name")
    };
}

/// Capture all CLI flags (e.g. `-a` or `--accept`) from the help message via regex
#[macro_export]
macro_rules! arg_regex_help {
    () => {
        Regex::new(r"^\s{2,6}(?:-(?<short>[a-zA-Z]),)?\s--(?<long>[a-zA-Z-]+)")
    };
}
