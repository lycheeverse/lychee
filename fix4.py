with open('lychee-bin/tests/cli.rs', 'r') as f:
    content = f.read()

insert = """
    #[tokio::test]
    async fn test_cli_input_url_status_error() {
        let mock_server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/success"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_string("https://example.com/ok"),
            )
            .mount(&mock_server)
            .await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/error"))
            .respond_with(
                wiremock::ResponseTemplate::new(404)
                    .set_body_string("https://example.com/not-found"),
            )
            .mount(&mock_server)
            .await;

        let server = mock_server.uri();
        let url_success = format!("{server}/success");
        let url_error = format!("{server}/error");

        let mut cmd = cargo_bin_cmd!();
        let assert = cmd.arg(url_success).arg(url_error).assert().failure();

        let output = String::from_utf8_lossy(&assert.get_output().stderr);

        // We should see an error for the error URL, but not for the success URL
        assert!(output.contains("Cannot read input content from URL: status code 404 Not Found. To check links in error pages, download and check locally instead."));
        assert!(!output.contains("Cannot read input content from URL: status code 200"));
    }
"""

content = content.replace("    #[tokio::test]\n    async fn test_legacy_cache_file_ignores_errors() -> Result<()> {",
insert + "\n    #[tokio::test]\n    async fn test_legacy_cache_file_ignores_errors() -> Result<()> {")

with open('lychee-bin/tests/cli.rs', 'w') as f:
    f.write(content)
