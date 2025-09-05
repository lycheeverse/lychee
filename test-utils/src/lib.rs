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
