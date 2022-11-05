#[cfg(test)]
mod cli {
    use assert_cmd::Command;
    use predicates::str::contains;
    use wiremock::{matchers::path, Mock, MockServer, Request, ResponseTemplate};

    fn main_command() -> Command {
        // this gets the "main" binary name (e.g. `lychee`)
        Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name")
    }

    #[tokio::test]
    async fn test_sitemap() {
        let mut cmd = main_command();

        let mock_server = MockServer::start().await;
        let uri = mock_server.uri();
        Mock::given(path("/"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;
        Mock::given(path("/foo.html"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;
        Mock::given(path("/bar.html"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;
        Mock::given(path("/sitemap.xml"))
            .respond_with(move |_req: &Request| {
                // Respond with a sitemap that contains links to the other pages
                let body = format!(
                    "
                    <?xml version=\"1.0\" encoding=\"UTF-8\"?>
                    <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">
                        <url>
                            <loc>{uri}/</loc>
                        </url>
                        <url>
                            <loc>{uri}/foo.html</loc>
                        </url>
                        <url>
                            <loc>{uri}/bar.html</loc>
                        </url>
                    </urlset>
                    ",
                );

                ResponseTemplate::new(200).set_body_string(body)
            })
            .mount(&mock_server)
            .await;

        let endpoint = mock_server.uri();

        cmd.arg("--recursive")
            .arg("--dump")
            .arg(&endpoint)
            .assert()
            .success()
            .stdout(contains(format!("{}/", endpoint)))
            .stdout(contains(format!("{}/foo.html", endpoint)))
            .stdout(contains(format!("{}/bar.html", endpoint)));
    }
}
