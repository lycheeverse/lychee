#[cfg(test)]
mod cli {
    use anyhow::anyhow;
    use assert_cmd::{assert::Assert, cargo::cargo_bin_cmd, output::OutputOkExt};
    use assert_json_diff::assert_json_include;
    use http::{Method, StatusCode};
    use lychee_lib::{InputSource, ResponseBody};
    use predicates::{
        prelude::PredicateBooleanExt,
        str::{contains, is_empty},
    };
    use pretty_assertions::assert_eq;
    use regex::Regex;
    use serde::Serialize;
    use serde_json::Value;
    use std::{
        collections::{HashMap, HashSet},
        error::Error,
        fs::{self, File},
        io::{BufRead, Write},
        path::Path,
        time::{Duration, Instant},
    };
    use tempfile::{NamedTempFile, tempdir};
    use test_utils::{fixtures_path, mock_server, redirecting_mock_server, root_path};

    use uuid::Uuid;
    use wiremock::{
        Mock, Request, ResponseTemplate,
        matchers::{basic_auth, method},
    };

    type Result<T> = std::result::Result<T, Box<dyn Error>>;

    // The lychee cache file name is used for some tests.
    // Since it is currently static and can't be overwritten, declare it as a
    // constant.
    const LYCHEE_CACHE_FILE: &str = ".lycheecache";

    /// Create a mock server which returns a 200 OK and a custom response body.
    macro_rules! mock_response {
        ($body:expr) => {{
            let mock_server = wiremock::MockServer::start().await;
            let template = wiremock::ResponseTemplate::new(200).set_body_string($body);
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .respond_with(template)
                .mount(&mock_server)
                .await;
            mock_server
        }};
    }

    /// Convert a relative path to an absolute path string
    /// starting from a base directory.
    fn path_str(base: &Path, relative_path: &str) -> String {
        base.join(relative_path).to_string_lossy().to_string()
    }

    /// Assert actual output lines equals to expected lines.
    /// Order of the lines is ignored.
    fn assert_lines_eq<S: AsRef<str> + Ord>(result: Assert, mut expected_lines: Vec<S>) {
        let output = &result.get_output().stdout;
        let mut actual_lines: Vec<String> = output
            .lines()
            .map(|line| line.unwrap().to_string())
            .collect();

        actual_lines.sort();
        expected_lines.sort();

        let expected_lines: Vec<String> = expected_lines
            .into_iter()
            .map(|l| l.as_ref().to_owned())
            .collect();

        assert_eq!(actual_lines, expected_lines);
    }

    /// Test the output of the JSON format.
    macro_rules! test_json_output {
        ($test_file:expr, $expected:expr $(, $arg:expr)*) => {{
            let mut cmd = cargo_bin_cmd!();
            let test_path = fixtures_path!().join($test_file);
            let outfile = format!("{}.json", uuid::Uuid::new_v4());

            let result = cmd$(.arg($arg))*.arg("--output").arg(&outfile).arg("--format").arg("json").arg(test_path).assert();

            let output = std::fs::read_to_string(&outfile)?;
            std::fs::remove_file(outfile)?;

            let actual: Value = serde_json::from_str(&output)?;
            let expected: Value = serde_json::to_value(&$expected)?;

            result.success();
            assert_json_include!(actual: actual, expected: expected);
            Ok(())
        }};
    }

    #[derive(Default, Serialize)]
    struct MockResponseStats {
        detailed_stats: bool,
        total: usize,
        successful: usize,
        unknown: usize,
        unsupported: usize,
        timeouts: usize,
        redirects: usize,
        excludes: usize,
        errors: usize,
        cached: usize,
        success_map: HashMap<InputSource, HashSet<ResponseBody>>,
        error_map: HashMap<InputSource, HashSet<ResponseBody>>,
        suggestion_map: HashMap<InputSource, HashSet<ResponseBody>>,
        excluded_map: HashMap<InputSource, HashSet<ResponseBody>>,
    }

    /// Test that the default report output format (compact) and mode (color)
    /// prints the failed URLs as well as their status codes on error. Make
    /// sure that the status code only occurs once.
    #[test]
    fn test_compact_output_format_contains_status() -> Result<()> {
        let test_path = fixtures_path!().join("TEST_INVALID_URLS.html");

        let mut cmd = cargo_bin_cmd!();
        cmd.arg("--format")
            .arg("compact")
            .arg("--mode")
            .arg("color")
            .arg(test_path)
            .env("FORCE_COLOR", "1")
            .assert()
            .failure()
            .code(2);

        let output = cmd.output()?;

        // Check that the output contains the status code (once) and the URL
        let output_str = String::from_utf8_lossy(&output.stdout);

        // The expected output is as follows:
        // "Find details below."
        // [EMPTY LINE]
        // [path/to/file]:
        //      [400] https://httpbin.org/status/404
        //      [500] https://httpbin.org/status/500
        //      [502] https://httpbin.org/status/502
        // (the order of the URLs may vary)

        // Check that the output contains the file path
        assert!(output_str.contains("TEST_INVALID_URLS.html"));

        let re = Regex::new(r"\s{5}\[\d{3}\] https://httpbin\.org/status/\d{3}").unwrap();
        let matches: Vec<&str> = re.find_iter(&output_str).map(|m| m.as_str()).collect();

        // Check that the status code occurs only once
        assert_eq!(matches.len(), 3);

        Ok(())
    }

    /// Test JSON output format
    #[tokio::test]
    async fn test_json_output() -> Result<()> {
        // Server that returns a bunch of 200 OK responses
        let mock_server_ok = mock_server!(StatusCode::OK);
        let mut cmd = cargo_bin_cmd!();
        cmd.arg("--format")
            .arg("json")
            .arg("-vv")
            .arg("--no-progress")
            .arg("-")
            .write_stdin(mock_server_ok.uri())
            .assert()
            .success();
        let output = cmd.output().unwrap();
        let output_json = serde_json::from_slice::<Value>(&output.stdout)?;

        // Check that the output is valid JSON
        assert!(output_json.is_object());
        // Check that the output contains the expected keys
        assert!(output_json.get("detailed_stats").is_some());
        assert!(output_json.get("success_map").is_some());
        assert!(output_json.get("error_map").is_some());
        assert!(output_json.get("excluded_map").is_some());

        // Check the success map
        let success_map = output_json["success_map"].as_object().unwrap();
        assert_eq!(success_map.len(), 1);

        // Get the actual URL from the mock server for comparison
        let mock_url = mock_server_ok.uri();

        // Create the expected success map structure
        let expected_success_map = serde_json::json!({
            "stdin": [
                {
                    "status": {
                        "code": 200,
                        "text": "200 OK"
                    },
                    "url": format!("{mock_url}/"),
                }
            ]
        });

        // Compare the actual success map with the expected one
        assert_eq!(
            success_map,
            expected_success_map.as_object().unwrap(),
            "Success map doesn't match expected structure"
        );

        Ok(())
    }

    /// JSON-formatted output should always be valid JSON.
    /// Additional hints and error messages should be printed to `stderr`.
    /// See https://github.com/lycheeverse/lychee/issues/1355
    #[test]
    fn test_valid_json_output_to_stdout_on_error() -> Result<()> {
        let test_path = fixtures_path!().join("TEST_GITHUB_404.md");

        let mut cmd = cargo_bin_cmd!();
        cmd.arg("--format")
            .arg("json")
            .arg(test_path)
            .assert()
            .failure()
            .code(2);

        let output = cmd.output()?;

        // Check that the output is valid JSON
        assert!(serde_json::from_slice::<Value>(&output.stdout).is_ok());
        Ok(())
    }

    #[test]
    fn test_detailed_json_output_on_error() -> Result<()> {
        let test_path = fixtures_path!().join("TEST_DETAILED_JSON_OUTPUT_ERROR.md");

        let mut cmd = cargo_bin_cmd!();
        cmd.arg("--format")
            .arg("json")
            .arg(&test_path)
            .assert()
            .failure()
            .code(2);

        let output = cmd.output()?;

        // Check that the output is valid JSON
        assert!(serde_json::from_slice::<Value>(&output.stdout).is_ok());

        // Parse site error status from the error_map
        let output_json = serde_json::from_slice::<Value>(&output.stdout).unwrap();
        let site_error_status =
            &output_json["error_map"][&test_path.to_str().unwrap()][0]["status"];

        assert_eq!(
            "SSL certificate expired. Site needs to renew certificate",
            site_error_status["details"]
        );
        Ok(())
    }

    #[test]
    fn test_exclude_all_private() -> Result<()> {
        test_json_output!(
            "TEST_ALL_PRIVATE.md",
            MockResponseStats {
                total: 7,
                excludes: 7,
                ..MockResponseStats::default()
            },
            "--exclude-all-private"
        )
    }

    #[test]
    fn test_local_directories() -> Result<()> {
        test_json_output!(
            "TEST_LOCAL_DIRECTORIES.md",
            MockResponseStats {
                total: 4,
                successful: 4,
                ..MockResponseStats::default()
            }
        )
    }

    #[test]
    fn test_email() -> Result<()> {
        test_json_output!(
            "TEST_EMAIL.md",
            MockResponseStats {
                total: 5,
                excludes: 0,
                successful: 5,
                ..MockResponseStats::default()
            },
            "--include-mail"
        )
    }

    #[test]
    fn test_exclude_email_by_default() -> Result<()> {
        test_json_output!(
            "TEST_EMAIL.md",
            MockResponseStats {
                total: 5,
                excludes: 3,
                successful: 2,
                ..MockResponseStats::default()
            }
        )
    }

    #[test]
    fn test_email_html_with_subject() {
        let input = fixtures_path!().join("TEST_EMAIL_QUERY_PARAMS.html");

        cargo_bin_cmd!()
            .arg("--dump")
            .arg(input)
            .arg("--include-mail")
            .assert()
            .success()
            .stdout(contains("hello@example.org?subject=%5BHello%5D"));
    }

    #[test]
    fn test_email_markdown_with_subject() {
        let input = fixtures_path!().join("TEST_EMAIL_QUERY_PARAMS.md");

        cargo_bin_cmd!()
            .arg("--dump")
            .arg(input)
            .arg("--include-mail")
            .assert()
            .success()
            .stdout(contains("hello@example.org?subject=%5BHello%5D"));
    }

    #[test]
    fn test_stylesheet_misinterpreted_as_email() -> Result<()> {
        test_json_output!(
            "TEST_STYLESHEET_LINK.md",
            MockResponseStats {
                total: 0,
                ..MockResponseStats::default()
            }
        )
    }

    /// Test that a GitHub link can be checked without specifying the token.
    #[test]
    fn test_check_github_no_token() -> Result<()> {
        test_json_output!(
            "TEST_GITHUB.md",
            MockResponseStats {
                total: 1,
                successful: 1,
                ..MockResponseStats::default()
            }
        )
    }

    /// Test unsupported URI schemes
    #[test]
    fn test_unsupported_uri_schemes_are_ignored() {
        let test_schemes_path = fixtures_path!().join("TEST_SCHEMES.txt");

        // Exclude file link because it doesn't exist on the filesystem.
        // (File URIs are absolute paths, which we don't have.)
        // Nevertheless, the `file` scheme should be recognized.
        cargo_bin_cmd!()
            .arg(test_schemes_path)
            .arg("--exclude")
            .arg("file://")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("3 Total"))
            .stdout(contains("1 OK"))
            .stdout(contains("1 Excluded"));
    }

    #[test]
    fn test_resolve_paths() {
        let dir = fixtures_path!().join("resolve_paths");

        cargo_bin_cmd!()
            .arg("--offline")
            .arg("--base-url")
            .arg(&dir)
            .arg(dir.join("index.html"))
            .env_clear()
            .assert()
            .success()
            .stdout(contains("3 Total"))
            .stdout(contains("3 OK"));
    }

    #[test]
    fn test_resolve_paths_from_root_dir() {
        let dir = fixtures_path!().join("resolve_paths_from_root_dir");

        cargo_bin_cmd!()
            .arg("--offline")
            .arg("--include-fragments")
            .arg("--root-dir")
            .arg(&dir)
            .arg(dir.join("nested").join("index.html"))
            .env_clear()
            .assert()
            .failure()
            .stdout(contains("7 Total"))
            .stdout(contains("5 OK"))
            .stdout(contains("2 Errors"));

        // test with a relative root-dir argument too
        cargo_bin_cmd!()
            .current_dir(dir.parent().unwrap())
            .arg("--offline")
            .arg("--include-fragments")
            .arg("--root-dir")
            .arg(dir.file_name().unwrap())
            .arg(dir.join("nested").join("index.html"))
            .env_clear()
            .assert()
            .failure()
            .stdout(contains("7 Total"))
            .stdout(contains("5 OK"))
            .stdout(contains("2 Errors"));
    }

    #[test]
    fn test_resolve_paths_from_root_dir_and_base_url() {
        let dir = fixtures_path!();

        cargo_bin_cmd!()
            .arg("--offline")
            .arg("--root-dir")
            .arg("/resolve_paths")
            .arg("--base-url")
            .arg(&dir)
            .arg(dir.join("resolve_paths").join("index.html"))
            .env_clear()
            .assert()
            .success()
            .stdout(contains("3 Total"))
            .stdout(contains("3 OK"));
    }

    #[test]
    fn test_resolve_paths_from_root_dir_and_local_base_url() {
        let dir = fixtures_path!();

        cargo_bin_cmd!()
            .arg("--dump")
            .arg("--root-dir")
            .arg("/root")
            .arg("--base-url")
            .arg("/base/")
            .arg(dir.join("resolve_paths").join("index2.html"))
            .env_clear()
            .assert()
            .success()
            .stdout(contains("file:///base/same%20page.html#x"))
            .stdout(contains("file:///base/root"))
            .stdout(contains("file:///base/root/another%20page#y"))
            .stdout(contains("file:///base/root/about"));
    }

    #[test]
    fn test_nonexistent_root_dir() {
        cargo_bin_cmd!()
            .arg("--root-dir")
            .arg("i don't exist blah blah")
            .arg("http://example.com")
            .assert()
            .failure()
            .stderr(contains("Invalid root directory"))
            .code(1);
    }

    #[test]
    fn test_youtube_quirk() {
        let url = "https://www.youtube.com/watch?v=NlKuICiT470&list=PLbWDhxwM_45mPVToqaIZNbZeIzFchsKKQ&index=7";

        cargo_bin_cmd!()
            .write_stdin(url)
            .arg("--verbose")
            .arg("--no-progress")
            .arg("-")
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 OK"));
    }

    #[test]
    fn test_crates_io_quirk() {
        let url = "https://crates.io/crates/lychee";

        cargo_bin_cmd!()
            .write_stdin(url)
            .arg("--verbose")
            .arg("--no-progress")
            .arg("-")
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 OK"));
    }

    #[test]
    // Exclude Twitter links because they require login to view tweets.
    // https://techcrunch.com/2023/06/30/twitter-now-requires-an-account-to-view-tweets/
    // https://github.com/zedeus/nitter/issues/919
    fn test_ignored_hosts() {
        let url = "https://twitter.com/zarfeblong/status/1339742840142872577";

        cargo_bin_cmd!()
            .write_stdin(url)
            .arg("--verbose")
            .arg("--no-progress")
            .arg("-")
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 Excluded"));
    }

    #[tokio::test]
    async fn test_failure_404_link() -> Result<()> {
        let mock_server = mock_server!(StatusCode::NOT_FOUND);
        let dir = tempfile::tempdir()?;
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path)?;
        writeln!(file, "{}", mock_server.uri())?;

        cargo_bin_cmd!()
            .arg(file_path)
            .write_stdin(mock_server.uri())
            .assert()
            .failure()
            .code(2);

        Ok(())
    }

    #[test]
    fn test_schemes() {
        let test_schemes_path = fixtures_path!().join("TEST_SCHEMES.md");

        cargo_bin_cmd!()
            .arg(test_schemes_path)
            .arg("--scheme")
            .arg("https")
            .arg("--scheme")
            .arg("http")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("3 Total"))
            .stdout(contains("2 OK"))
            .stdout(contains("1 Excluded"));
    }

    #[test]
    fn test_caching_single_file() {
        // Repetitions in one file shall all be checked and counted only once.
        let test_schemes_path_1 = fixtures_path!().join("TEST_REPETITION_1.txt");

        cargo_bin_cmd!()
            .arg(&test_schemes_path_1)
            .env_clear()
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 OK"));
    }

    #[test]
    // Test that two identical requests don't get executed twice.
    fn test_caching_across_files() -> Result<()> {
        // Repetitions across multiple files shall all be checked only once.
        let repeated_uris = fixtures_path!().join("TEST_REPETITION_*.txt");

        test_json_output!(
            repeated_uris,
            MockResponseStats {
                total: 2,
                cached: 1,
                successful: 2,
                excludes: 0,
                ..MockResponseStats::default()
            },
            // Two requests to the same URI may be executed in parallel. As a
            // result, the response might not be cached and the test would be
            // flaky. Therefore limit the concurrency to one request at a time.
            "--max-concurrency",
            "1"
        )
    }

    #[test]
    fn test_failure_github_404_no_token() {
        let test_github_404_path = fixtures_path!().join("TEST_GITHUB_404.md");

        cargo_bin_cmd!()
            .arg(test_github_404_path)
            .arg("--no-progress")
            .env_clear()
            .assert()
            .failure()
            .code(2)
            .stdout(contains(
                r#"[404] https://github.com/mre/idiomatic-rust-doesnt-exist-man | Rejected status code (this depends on your "accept" configuration): Not Found"#
            ))
            .stderr(contains(
                "There were issues with GitHub URLs. You could try setting a GitHub token and running lychee again.",
            ));
    }

    #[tokio::test]
    async fn test_stdin_input() {
        let mock_server = mock_server!(StatusCode::OK);

        cargo_bin_cmd!()
            .arg("-")
            .write_stdin(mock_server.uri())
            .assert()
            .success();
    }

    #[tokio::test]
    async fn test_stdin_input_failure() {
        let mock_server = mock_server!(StatusCode::INTERNAL_SERVER_ERROR);

        cargo_bin_cmd!()
            .arg("-")
            .write_stdin(mock_server.uri())
            .assert()
            .failure()
            .code(2);
    }

    #[tokio::test]
    async fn test_stdin_input_multiple() {
        let mock_server_a = mock_server!(StatusCode::OK);
        let mock_server_b = mock_server!(StatusCode::OK);

        // this behavior (treating multiple `-` as separate inputs) is the same as most CLI tools
        // that accept `-` as stdin, e.g. `cat`, `bat`, `grep` etc.
        cargo_bin_cmd!()
            .arg("-")
            .arg("-")
            .write_stdin(mock_server_a.uri())
            .write_stdin(mock_server_b.uri())
            .assert()
            .success();
    }

    #[test]
    fn test_missing_file_ok_if_skip_missing() {
        let filename = format!("non-existing-file-{}", uuid::Uuid::new_v4());
        cargo_bin_cmd!()
            .arg(&filename)
            .arg("--skip-missing")
            .assert()
            .success();
    }

    #[test]
    fn test_skips_hidden_files_by_default() {
        cargo_bin_cmd!()
            .arg(fixtures_path!().join("hidden/"))
            .assert()
            .success()
            .stdout(contains("0 Total"));

        cargo_bin_cmd!()
            .arg("--dump")
            .arg(fixtures_path!().join("hidden/"))
            .assert()
            .stdout("")
            .success();

        cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg(fixtures_path!().join("hidden/"))
            .assert()
            .stdout("")
            .success();
    }

    #[test]
    fn test_include_hidden_file() {
        cargo_bin_cmd!()
            .arg(fixtures_path!().join("hidden/"))
            .arg("--hidden")
            .assert()
            .success()
            .stdout(contains("2 Total"));

        let result = cargo_bin_cmd!()
            .arg("--dump")
            .arg("--hidden")
            .arg(fixtures_path!().join("hidden/"))
            .assert()
            .success();

        assert_lines_eq(
            result,
            vec!["https://rust-lang.org/", "https://rust-lang.org/"],
        );
    }

    #[test]
    fn test_skips_ignored_files_by_default() {
        cargo_bin_cmd!()
            .arg(fixtures_path!().join("ignore/"))
            .assert()
            .success()
            .stdout(contains("0 Total"));

        cargo_bin_cmd!()
            .arg("--dump")
            .arg(fixtures_path!().join("ignore/"))
            .assert()
            .success()
            .stdout("");

        cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg(fixtures_path!().join("ignore/"))
            .assert()
            .success()
            .stdout("");
    }

    #[test]
    fn test_include_ignored_file() {
        cargo_bin_cmd!()
            .arg(fixtures_path!().join("ignore/"))
            .arg("--no-ignore")
            .assert()
            .success()
            .stdout(contains("1 Total"));

        cargo_bin_cmd!()
            .arg("--dump")
            .arg("--no-ignore")
            .arg(fixtures_path!().join("ignore/"))
            .assert()
            .success()
            .stdout(contains("wikipedia.org"));

        cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg("--no-ignore")
            .arg(fixtures_path!().join("ignore/"))
            .assert()
            .success()
            .stdout(contains("ignored-file.md"));
    }

    #[tokio::test]
    async fn test_glob() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mock_server_a = mock_server!(StatusCode::OK);
        let mock_server_b = mock_server!(StatusCode::OK);
        let mut file_a = File::create(dir.path().join("a.md"))?;
        let mut file_b = File::create(dir.path().join("b.md"))?;

        writeln!(file_a, "{}", mock_server_a.uri().as_str())?;
        writeln!(file_b, "{}", mock_server_b.uri().as_str())?;

        cargo_bin_cmd!()
            .arg(dir.path().join("*.md"))
            .arg("--verbose")
            .assert()
            .success()
            .stdout(contains("2 Total"));

        Ok(())
    }

    #[cfg(target_os = "linux")] // MacOS and Windows have case-insensitive filesystems
    #[tokio::test]
    async fn test_glob_ignore_case() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mock_server_a = mock_server!(StatusCode::OK);
        let mock_server_b = mock_server!(StatusCode::OK);
        let mut file_a = File::create(dir.path().join("README.md"))?;
        let mut file_b = File::create(dir.path().join("readme.md"))?;

        writeln!(file_a, "{}", mock_server_a.uri().as_str())?;
        writeln!(file_b, "{}", mock_server_b.uri().as_str())?;

        cargo_bin_cmd!()
            .arg(dir.path().join("[r]eadme.md"))
            .arg("--verbose")
            .arg("--glob-ignore-case")
            .assert()
            .success()
            .stdout(contains("2 Total"));

        Ok(())
    }

    #[tokio::test]
    async fn test_glob_recursive() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let subdir_level_1 = tempfile::tempdir_in(&dir)?;
        let subdir_level_2 = tempfile::tempdir_in(&subdir_level_1)?;

        let mock_server = mock_server!(StatusCode::OK);
        let mut file = File::create(subdir_level_2.path().join("test.md"))?;

        writeln!(file, "{}", mock_server.uri().as_str())?;

        cargo_bin_cmd!()
            .arg(dir.path().join("**/*.md")) // ** should be a recursive glob
            .arg("--verbose")
            .assert()
            .success()
            .stdout(contains("1 Total"));

        Ok(())
    }

    /// Test formatted file output
    #[test]
    fn test_formatted_file_output() -> Result<()> {
        test_json_output!(
            "TEST.md",
            MockResponseStats {
                total: 12,
                successful: 10,
                excludes: 2,
                ..MockResponseStats::default()
            }
        )
    }

    /// Test writing output of `--dump` command to file
    #[test]
    fn test_dump_to_file() -> Result<()> {
        let test_path = fixtures_path!().join("TEST.md");
        let outfile = format!("{}", Uuid::new_v4());

        cargo_bin_cmd!()
            .arg("--output")
            .arg(&outfile)
            .arg("--dump")
            .arg("--include-mail")
            .arg(test_path)
            .assert()
            .success();

        let output = fs::read_to_string(&outfile)?;

        // We expect 11 links in the test file
        // Running the command from the command line will print 9 links,
        // because the actual `--dump` command filters out the two
        // http(s)://example.com links
        assert_eq!(output.lines().count(), 12);
        fs::remove_file(outfile)?;
        Ok(())
    }

    /// Test excludes
    #[test]
    fn test_exclude_wildcard() -> Result<()> {
        let test_path = fixtures_path!().join("TEST.md");

        cargo_bin_cmd!()
            .arg(test_path)
            .arg("--exclude")
            .arg(".*")
            .assert()
            .success()
            .stdout(contains("12 Excluded"));

        Ok(())
    }

    #[test]
    fn test_exclude_multiple_urls() -> Result<()> {
        let test_path = fixtures_path!().join("TEST.md");

        cargo_bin_cmd!()
            .arg(test_path)
            .arg("--exclude")
            .arg("https://en.wikipedia.org/*")
            .arg("--exclude")
            .arg("https://ldra.com/")
            .assert()
            .success()
            .stdout(contains("4 Excluded"));

        Ok(())
    }

    #[tokio::test]
    async fn test_empty_config() {
        let mock_server = mock_server!(StatusCode::OK);
        let config = fixtures_path!().join("configs").join("empty.toml");
        cargo_bin_cmd!()
            .arg("--config")
            .arg(config)
            .arg("-")
            .write_stdin(mock_server.uri())
            .env_clear()
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 OK"));
    }

    #[test]
    fn test_invalid_default_config() {
        let test_path = fixtures_path!().join("configs");
        let mut cmd = cargo_bin_cmd!();
        cmd.current_dir(test_path)
            .arg(".")
            .assert()
            .failure()
            .stderr(contains("Cannot load default configuration file"));
    }

    #[tokio::test]
    async fn test_include_mail_config() -> Result<()> {
        let test_mail_address = "mailto:hello-test@testingabc.io";

        let mut config = NamedTempFile::new()?;
        writeln!(config, "include_mail = false")?;

        cargo_bin_cmd!()
            .arg("--config")
            .arg(config.path().to_str().unwrap())
            .arg("-")
            .write_stdin(test_mail_address)
            .env_clear()
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 Excluded"));

        let mut config = NamedTempFile::new()?;
        writeln!(config, "include_mail = true")?;

        cargo_bin_cmd!()
            .arg("--config")
            .arg(config.path().to_str().unwrap())
            .arg("-")
            .write_stdin(test_mail_address)
            .env_clear()
            .assert()
            .failure()
            .stdout(contains("1 Total"))
            .stdout(contains("1 Error"));

        Ok(())
    }

    #[tokio::test]
    async fn test_cache_config() -> Result<()> {
        let mock_server = mock_server!(StatusCode::OK);
        let config = fixtures_path!().join("configs").join("cache.toml");
        cargo_bin_cmd!()
            .arg("--config")
            .arg(config)
            .arg("-")
            .write_stdin(mock_server.uri())
            .env_clear()
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 OK"));

        Ok(())
    }

    #[tokio::test]
    async fn test_invalid_config() {
        let config = fixtures_path!().join("configs").join("invalid.toml");
        cargo_bin_cmd!()
            .arg("--config")
            .arg(config)
            .arg("-")
            .env_clear()
            .assert()
            .failure()
            .stderr(contains("Cannot load configuration file"))
            .stderr(contains("Failed to parse"))
            .stderr(contains("TOML parse error"));
    }

    #[tokio::test]
    async fn test_config_invalid_keys() {
        let mock_server = mock_server!(StatusCode::OK);
        let config = fixtures_path!().join("configs").join("invalid-key.toml");
        cargo_bin_cmd!()
            .arg("--config")
            .arg(config)
            .arg("-")
            .write_stdin(mock_server.uri())
            .env_clear()
            .assert()
            .failure()
            .code(3)
            .stderr(contains("unknown field `this_is_invalid`, expected one of"));
    }

    #[tokio::test]
    async fn test_missing_config_error() {
        let mock_server = mock_server!(StatusCode::OK);
        cargo_bin_cmd!()
            .arg("--config")
            .arg("config.does.not.exist.toml")
            .arg("-")
            .write_stdin(mock_server.uri())
            .env_clear()
            .assert()
            .failure();
    }

    #[tokio::test]
    async fn test_config_example() {
        let mock_server = mock_server!(StatusCode::OK);
        let config = root_path!().join("lychee.example.toml");
        cargo_bin_cmd!()
            .arg("--config")
            .arg(config)
            .arg("-")
            .write_stdin(mock_server.uri())
            .env_clear()
            .assert()
            .success();
    }

    #[test]
    #[cfg(unix)]
    fn test_all_arguments_in_config() -> Result<()> {
        let help_cmd = cargo_bin_cmd!()
            .env_clear()
            .arg("--help")
            .assert()
            .success();
        let help_text = std::str::from_utf8(&help_cmd.get_output().stdout)?;

        let regex = test_utils::arg_regex_help!()?;
        let excluded = [
            "base",         // deprecated
            "exclude_file", // deprecated
            "config",       // not part of config
            "quiet",        // not part of config
            "help",         // special clap argument
            "version",      // special clap argument
        ];

        let arguments: Vec<String> = help_text
            .lines()
            .filter_map(|line| {
                let captures = regex.captures(line)?;
                captures.name("long").map(|m| m.as_str())
            })
            .map(|arg| arg.replace("-", "_"))
            .filter(|arg| !excluded.contains(&arg.as_str()))
            .collect();

        let config = root_path!().join("lychee.example.toml");
        let values: toml::Table = toml::from_str(&std::fs::read_to_string(config)?)?;

        for argument in arguments {
            if !values.contains_key(&argument) {
                panic!(
                    "Key '{argument}' missing in config.
The config file should contain every possible key for documentation purposes."
                )
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_config_smoketest() {
        let mock_server = mock_server!(StatusCode::OK);
        let config = fixtures_path!().join("configs").join("smoketest.toml");
        cargo_bin_cmd!()
            .arg("--config")
            .arg(config)
            .arg("-")
            .write_stdin(mock_server.uri())
            .env_clear()
            .assert()
            .success();
    }

    #[tokio::test]
    async fn test_config_accept() {
        let mock_server = mock_server!(StatusCode::OK);
        let config = fixtures_path!().join("configs").join("accept.toml");
        cargo_bin_cmd!()
            .arg("--config")
            .arg(config)
            .arg("-")
            .write_stdin(mock_server.uri())
            .env_clear()
            .assert()
            .success();
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_config_files_from() {
        let dir = fixtures_path!().join("configs").join("files_from");
        let result = cargo_bin_cmd!()
            .current_dir(dir)
            .arg("/dev/null") // at least one input arg is required. this could be changed in the future
            .arg("--dump")
            .assert()
            .success();

        assert_lines_eq(result, vec!["https://wikipedia.org/"]);
    }

    #[test]
    fn test_lycheeignore_file() -> Result<()> {
        let test_path = fixtures_path!().join("lycheeignore");

        let cmd = cargo_bin_cmd!()
            .current_dir(test_path)
            .arg("--dump")
            .arg("TEST.md")
            .assert()
            .stdout(contains("https://example.com"))
            .stdout(contains("https://example.com/bar"))
            .stdout(contains("https://example.net"));

        let output = cmd.get_output();
        let output = std::str::from_utf8(&output.stdout).unwrap();
        assert_eq!(output.lines().count(), 3);

        Ok(())
    }

    #[test]
    fn test_lycheeignore_and_exclude_file() -> Result<()> {
        let test_path = fixtures_path!().join("lycheeignore");
        let excludes_path = test_path.join("normal-exclude-file");

        cargo_bin_cmd!()
            .current_dir(test_path)
            .arg("TEST.md")
            .arg("--exclude-file")
            .arg(excludes_path)
            .assert()
            .success()
            .stdout(contains("8 Total"))
            .stdout(contains("6 Excluded"));

        Ok(())
    }

    #[tokio::test]
    async fn test_lycheecache_file() -> Result<()> {
        let base_path = fixtures_path!().join("cache");
        let cache_file = base_path.join(LYCHEE_CACHE_FILE);

        // Ensure clean state
        if cache_file.exists() {
            println!("Removing cache file before test: {cache_file:?}");
            fs::remove_file(&cache_file)?;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Setup mock servers
        let mock_server_ok = mock_server!(StatusCode::OK);
        let mock_server_err = mock_server!(StatusCode::NOT_FOUND);
        let mock_server_exclude = mock_server!(StatusCode::OK);

        // Create test file
        let dir = tempfile::tempdir()?;
        let file_path = dir.path().join("c.md");
        let mut file = File::create(&file_path)?;
        writeln!(file, "{}", mock_server_ok.uri().as_str())?;
        writeln!(file, "{}", mock_server_err.uri().as_str())?;
        writeln!(file, "{}", mock_server_exclude.uri().as_str())?;
        file.sync_all()?;

        // Create and run command
        let mut cmd = cargo_bin_cmd!();
        cmd.current_dir(&base_path)
            .arg(&file_path)
            .arg("--verbose")
            .arg("--no-progress")
            .arg("--cache")
            .arg("--exclude")
            .arg(mock_server_exclude.uri());

        // Note: Don't check output.status.success() since we expect
        // a non-zero exit code (2) when lychee finds broken links
        let _output = cmd.output()?;

        // Wait for cache file to be written
        for _ in 0..20 {
            if cache_file.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Check cache contents
        let data = fs::read_to_string(&cache_file)?;
        println!("Cache file contents: {data}");

        assert!(
            data.contains(&format!("{}/,200", mock_server_ok.uri())),
            "Missing OK entry in cache"
        );
        assert!(
            data.contains(&format!("{}/,404", mock_server_err.uri())),
            "Missing error entry in cache"
        );

        // Run again to verify cache behavior
        cmd.assert()
            .stderr(contains(format!(
                "[200] {}/ | OK (cached)\n",
                mock_server_ok.uri()
            )))
            .stderr(contains(format!(
                "[404] {}/ | Error (cached)\n",
                mock_server_err.uri()
            )));

        // Clean up
        fs::remove_file(&cache_file).map_err(|e| {
            anyhow::anyhow!("Failed to remove cache file: {cache_file:?}, error: {e}")
        })?;

        Ok(())
    }

    #[tokio::test]
    async fn test_lycheecache_exclude_custom_status_codes() -> Result<()> {
        let base_path = fixtures_path!().join("cache");
        let cache_file = base_path.join(LYCHEE_CACHE_FILE);

        // Unconditionally remove cache file if it exists
        let _ = fs::remove_file(&cache_file);

        let mock_server_ok = mock_server!(StatusCode::OK);
        let mock_server_no_content = mock_server!(StatusCode::NO_CONTENT);
        let mock_server_too_many_requests = mock_server!(StatusCode::TOO_MANY_REQUESTS);

        let dir = tempfile::tempdir()?;
        let mut file = File::create(dir.path().join("c.md"))?;

        writeln!(file, "{}", mock_server_ok.uri().as_str())?;
        writeln!(file, "{}", mock_server_no_content.uri().as_str())?;
        writeln!(file, "{}", mock_server_too_many_requests.uri().as_str())?;

        let mut cmd = cargo_bin_cmd!();
        let test_cmd = cmd
            .current_dir(&base_path)
            .arg(dir.path().join("c.md"))
            .arg("--verbose")
            .arg("--no-progress")
            .arg("--cache")
            .arg("--cache-exclude-status")
            .arg("204,429");

        assert!(
            !cache_file.exists(),
            "cache file should not exist before this test"
        );

        // Run first without cache to generate the cache file
        test_cmd
            .assert()
            .stderr(contains(format!("[200] {}/\n", mock_server_ok.uri())))
            .stderr(contains(format!(
                "[204] {}/ | 204 No Content: No Content\n",
                mock_server_no_content.uri()
            )))
            .stderr(contains(format!(
                "[429] {}/ | Rejected status code (this depends on your \"accept\" configuration): Too Many Requests\n",
                mock_server_too_many_requests.uri()
            )));

        // Check content of cache file
        let data = fs::read_to_string(&cache_file)?;

        if data.is_empty() {
            println!("Cache file is empty!");
        }

        assert!(data.contains(&format!("{}/,200", mock_server_ok.uri())));
        assert!(!data.contains(&format!("{}/,204", mock_server_no_content.uri())));
        assert!(!data.contains(&format!("{}/,429", mock_server_too_many_requests.uri())));

        // Unconditionally remove the cache file
        let _ = fs::remove_file(&cache_file);
        Ok(())
    }

    #[tokio::test]
    async fn test_lycheecache_accept_custom_status_codes() -> Result<()> {
        let base_path = fixtures_path!().join("cache_accept_custom_status_codes");
        let cache_file = base_path.join(LYCHEE_CACHE_FILE);

        // Unconditionally remove cache file if it exists
        let _ = fs::remove_file(&cache_file);

        let mock_server_ok = mock_server!(StatusCode::OK);
        let mock_server_teapot = mock_server!(StatusCode::IM_A_TEAPOT);
        let mock_server_server_error = mock_server!(StatusCode::INTERNAL_SERVER_ERROR);

        let dir = tempfile::tempdir()?;
        let mut file = File::create(dir.path().join("c.md"))?;

        writeln!(file, "{}", mock_server_ok.uri().as_str())?;
        writeln!(file, "{}", mock_server_teapot.uri().as_str())?;
        writeln!(file, "{}", mock_server_server_error.uri().as_str())?;

        let mut cmd = cargo_bin_cmd!();
        let test_cmd = cmd
            .current_dir(&base_path)
            .arg(dir.path().join("c.md"))
            .arg("--verbose")
            .arg("--cache");

        assert!(
            !cache_file.exists(),
            "cache file should not exist before this test"
        );

        // run first without cache to generate the cache file
        // ignore exit code
        test_cmd
            .assert()
            .failure()
            .code(2)
            .stdout(contains(format!(
                r#"[418] {}/ | Rejected status code (this depends on your "accept" configuration): I'm a teapot"#,
                mock_server_teapot.uri()
            )))
            .stdout(contains(format!(
                r#"[500] {}/ | Rejected status code (this depends on your "accept" configuration): Internal Server Error"#,
                mock_server_server_error.uri()
            )));

        // check content of cache file
        let data = fs::read_to_string(&cache_file)?;
        assert!(data.contains(&format!("{}/,200", mock_server_ok.uri())));
        assert!(data.contains(&format!("{}/,418", mock_server_teapot.uri())));
        assert!(data.contains(&format!("{}/,500", mock_server_server_error.uri())));

        // run again to verify cache behavior
        // this time accept 418 and 500 as valid status codes
        test_cmd
            .arg("--no-progress")
            .arg("--accept")
            .arg("418,500")
            .assert()
            .success()
            .stderr(contains(format!(
                "[418] {}/ | OK (cached)",
                mock_server_teapot.uri()
            )))
            .stderr(contains(format!(
                "[500] {}/ | OK (cached)",
                mock_server_server_error.uri()
            )));

        // clear the cache file
        fs::remove_file(&cache_file)?;

        Ok(())
    }

    #[tokio::test]
    async fn test_accept_overrides_defaults_not_additive() -> Result<()> {
        let mock_server_200 = mock_server!(StatusCode::OK);

        cargo_bin_cmd!()
            .arg("--accept")
            .arg("404") // ONLY accept 404 - should reject 200 as we overwrite the default
            .arg("-")
            .write_stdin(mock_server_200.uri())
            .assert()
            .failure()
            .code(2)
            .stdout(contains(format!(
                r#"[200] {}/ | Rejected status code (this depends on your "accept" configuration): OK"#,
                mock_server_200.uri()
            )));

        Ok(())
    }

    #[tokio::test]
    async fn test_skip_cache_unsupported() -> Result<()> {
        let base_path = fixtures_path!().join("cache");
        let cache_file = base_path.join(LYCHEE_CACHE_FILE);

        // Unconditionally remove cache file if it exists
        let _ = fs::remove_file(&cache_file);

        let unsupported_url = "slack://user".to_string();
        let excluded_url = "https://example.com/";

        // run first without cache to generate the cache file
        cargo_bin_cmd!()
            .current_dir(&base_path)
            .write_stdin(format!("{unsupported_url}\n{excluded_url}"))
            .arg("--cache")
            .arg("--verbose")
            .arg("--no-progress")
            .arg("--exclude")
            .arg(excluded_url)
            .arg("--")
            .arg("-")
            .assert()
            .stderr(contains(format!(
                "[IGNORED] {unsupported_url} | Unsupported: Error creating request client"
            )))
            .stderr(contains(format!("[EXCLUDED] {excluded_url}\n")));

        // The cache file should be empty, because the only checked URL is
        // unsupported and we don't want to cache that. It might be supported in
        // future versions.
        let buf = fs::read(&cache_file).unwrap();
        assert!(buf.is_empty());

        // clear the cache file
        fs::remove_file(&cache_file)?;

        Ok(())
    }

    /// Unknown status codes should be skipped and not cached by default
    /// The reason is that we don't know if they are valid or not
    /// and even if they are invalid, we don't know if they will be valid in the
    /// future.
    ///
    /// Since we cannot test this with our mock server (because hyper panics on
    /// invalid status codes) we use LinkedIn as a test target.
    ///
    /// Unfortunately, LinkedIn does not always return 999, so this is a flaky
    /// test. We only check that the cache file doesn't contain any invalid
    /// status codes.
    #[tokio::test]
    async fn test_skip_cache_unknown_status_code() -> Result<()> {
        let base_path = fixtures_path!().join("cache");
        let cache_file = base_path.join(LYCHEE_CACHE_FILE);

        // Unconditionally remove cache file if it exists
        let _ = fs::remove_file(&cache_file);

        // https://linkedin.com returns 999 for unknown status codes
        // use this as a test target
        let unknown_url = "https://www.linkedin.com/company/corrode";

        // run first without cache to generate the cache file
        cargo_bin_cmd!()
            .current_dir(&base_path)
            .write_stdin(unknown_url.to_string())
            .arg("--cache")
            .arg("--verbose")
            .arg("--no-progress")
            .arg("--")
            .arg("-")
            .assert()
            .success();

        // If the status code was 999, the cache file should be empty
        // because we do not want to cache unknown status codes
        let buf = fs::read(&cache_file).unwrap();
        if !buf.is_empty() {
            let data = String::from_utf8(buf)?;
            // The cache file should not contain any invalid status codes
            // In that case, we expect a single entry with status code 200
            assert!(!data.contains("999"));
            assert!(data.contains("200"));
        }

        // clear the cache file
        fs::remove_file(&cache_file)?;

        Ok(())
    }

    #[tokio::test]
    async fn test_process_internal_host_caching() -> Result<()> {
        // Note that this process-internal per-host caching
        // has no direct relation to the lychee cache file
        // where state can be persisted between multiple invocations.
        let server = wiremock::MockServer::start().await;

        // Return one rate-limited response to make sure that
        // such a response isn't cached.
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let temp_dir = tempfile::tempdir()?;
        for i in 0..9 {
            let test_md1 = temp_dir.path().join(format!("test{i}.md"));
            fs::write(&test_md1, server.uri())?;
        }

        cargo_bin_cmd!()
            .arg(temp_dir.path())
            .arg("--host-stats")
            .assert()
            .success()
            .stdout(contains("9 Total"))
            .stdout(contains("9 OK"))
            .stdout(contains("0 Errors"))
            // Per-host statistics
            // 1 rate limited + 9 OK
            .stdout(contains("10 reqs"))
            // 1 rate limited, 1 OK, 8 cached
            .stdout(contains("80.0% cached"));

        server.verify().await;
        Ok(())
    }

    #[test]
    fn test_verbatim_skipped_by_default() {
        let input = fixtures_path!().join("TEST_CODE_BLOCKS.md");

        cargo_bin_cmd!()
            .arg(input)
            .arg("--dump")
            .assert()
            .success()
            .stdout(is_empty());
    }

    #[test]
    fn test_include_verbatim() {
        let input = fixtures_path!().join("TEST_CODE_BLOCKS.md");

        cargo_bin_cmd!()
            .arg("--include-verbatim")
            .arg(input)
            .arg("--dump")
            .assert()
            .success()
            .stdout(contains("http://127.0.0.1/block"))
            .stdout(contains("http://127.0.0.1/inline"))
            .stdout(contains("http://127.0.0.1/bash"));
    }
    #[tokio::test]
    async fn test_verbatim_skipped_by_default_via_file() {
        let file = fixtures_path!().join("TEST_VERBATIM.html");

        cargo_bin_cmd!()
            .arg("--dump")
            .arg(file)
            .assert()
            .success()
            .stdout(is_empty());
    }

    #[tokio::test]
    async fn test_verbatim_skipped_by_default_via_remote_url() {
        let file = fixtures_path!().join("TEST_VERBATIM.html");
        let body = fs::read_to_string(file).unwrap();
        let mock_server = mock_response!(body);

        cargo_bin_cmd!()
            .arg("--dump")
            .arg(mock_server.uri())
            .assert()
            .success()
            .stdout(is_empty());
    }

    #[tokio::test]
    async fn test_include_verbatim_via_remote_url() {
        let file = fixtures_path!().join("TEST_VERBATIM.html");
        let body = fs::read_to_string(file).unwrap();
        let mock_server = mock_response!(body);

        cargo_bin_cmd!()
            .arg("--include-verbatim")
            .arg("--dump")
            .arg(mock_server.uri())
            .assert()
            .success()
            .stdout(contains("http://www.example.com/pre"))
            .stdout(contains("http://www.example.com/code"))
            .stdout(contains("http://www.example.com/samp"))
            .stdout(contains("http://www.example.com/kbd"))
            .stdout(contains("http://www.example.com/var"))
            .stdout(contains("http://www.example.com/script"));
    }

    #[test]
    fn test_require_https() {
        let test_path = fixtures_path!().join("TEST_HTTP.html");
        cargo_bin_cmd!().arg(&test_path).assert().success();

        cargo_bin_cmd!()
            .arg("--require-https")
            .arg(test_path)
            .assert()
            .failure()
            .stdout(contains("This URI is available in HTTPS protocol, but HTTP is provided. Use 'https://example.com/' instead"));
    }

    /// If `base-dir` is not set, an error should be thrown if we encounter
    /// an absolute local link (e.g. `/about`) within a file.
    #[test]
    fn test_absolute_local_links_without_base() {
        let offline_dir = fixtures_path!().join("offline");

        cargo_bin_cmd!()
            .arg("--offline")
            .arg(offline_dir.join("index.html"))
            .env_clear()
            .assert()
            .failure()
            .stdout(contains("5 Error"))
            .stdout(contains("Error building URL").count(5));
    }

    #[test]
    fn test_inputs_without_scheme() {
        let test_path = fixtures_path!().join("TEST_HTTP.html");
        cargo_bin_cmd!()
            .arg("--dump")
            .arg("example.com")
            .arg(&test_path)
            .arg("https://example.org")
            .assert()
            .success();
    }

    #[test]
    fn test_print_excluded_links_in_verbose_mode() {
        let test_path = fixtures_path!().join("TEST_DUMP_EXCLUDE.txt");
        cargo_bin_cmd!()
            .arg("--dump")
            .arg("--verbose")
            .arg("--exclude")
            .arg("example.com")
            .arg("--")
            .arg(&test_path)
            .assert()
            .success()
            .stdout(contains(format!(
                "https://example.com/ ({}) [excluded]",
                test_path.display()
            )))
            .stdout(contains(format!(
                "https://example.org/ ({})",
                test_path.display()
            )))
            .stdout(contains(format!(
                "https://example.com/foo/bar ({}) [excluded]",
                test_path.display()
            )));
    }

    #[test]
    fn test_remap_uri() {
        cargo_bin_cmd!()
            .arg("--dump")
            .arg("--remap")
            .arg("https://example.com http://127.0.0.1:8080")
            .arg("--remap")
            .arg("https://example.org https://staging.example.com")
            .arg("--")
            .arg("-")
            .write_stdin("https://example.com\nhttps://example.org\nhttps://example.net\n")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("http://127.0.0.1:8080/"))
            .stdout(contains("https://staging.example.com/"))
            .stdout(contains("https://example.net/"));
    }

    #[test]
    #[ignore = "Skipping test until https://github.com/robinst/linkify/pull/58 is merged"]
    fn test_remap_path() {
        cargo_bin_cmd!()
            .arg("--dump")
            .arg("--remap")
            .arg("../../issues https://github.com/usnistgov/OSCAL/issues")
            .arg("--")
            .arg("-")
            .write_stdin("../../issues\n")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("https://github.com/usnistgov/OSCAL/issues"));
    }

    #[test]
    fn test_remap_capture() {
        cargo_bin_cmd!()
            .arg("--dump")
            .arg("--remap")
            .arg("https://example.com/(.*) http://example.org/$1")
            .arg("--")
            .arg("-")
            .write_stdin("https://example.com/foo\n")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("http://example.org/foo"));
    }

    #[test]
    fn test_remap_named_capture() {
        cargo_bin_cmd!()
            .arg("--dump")
            .arg("--remap")
            .arg("https://github.com/(?P<org>.*)/(?P<repo>.*) https://gitlab.com/$org/$repo")
            .arg("--")
            .arg("-")
            .write_stdin("https://github.com/lycheeverse/lychee\n")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("https://gitlab.com/lycheeverse/lychee"));
    }

    #[test]
    fn test_excluded_paths_regex() {
        let test_path = fixtures_path!().join("exclude-path");
        let excluded_path_1 = "\\/excluded?\\/"; // exclude paths containing a directory "exclude" and "excluded"
        let excluded_path_2 = "(\\.mdx|\\.txt)$"; // exclude .mdx and .txt files
        let result = cargo_bin_cmd!()
            .arg("--exclude-path")
            .arg(excluded_path_1)
            .arg("--exclude-path")
            .arg(excluded_path_2)
            .arg("--dump")
            .arg("--")
            .arg(&test_path)
            .assert()
            .success();

        assert_lines_eq(
            result,
            vec![
                "https://test.md/to-be-included-outer",
                "https://test.md/to-be-included-inner",
            ],
        );
    }

    #[test]
    fn test_handle_relative_paths_as_input() {
        let test_path = fixtures_path!();

        cargo_bin_cmd!()
            .current_dir(&test_path)
            .arg("--verbose")
            .arg("--exclude")
            .arg("example.*")
            .arg("--")
            .arg("./TEST_DUMP_EXCLUDE.txt")
            .assert()
            .success()
            .stdout(contains("3 Total"))
            .stdout(contains("3 Excluded"));
    }

    #[test]
    fn test_handle_nonexistent_relative_paths_as_input() {
        let test_path = fixtures_path!();

        cargo_bin_cmd!()
            .current_dir(&test_path)
            .arg("--verbose")
            .arg("--exclude")
            .arg("example.*")
            .arg("--")
            .arg("./NOT-A-REAL-TEST-FIXTURE.md")
            .assert()
            .failure()
            .stderr(contains("Invalid file path: ./NOT-A-REAL-TEST-FIXTURE.md"));
    }

    #[test]
    fn test_prevent_too_many_redirects() {
        let url = "https://http.codes/308";

        cargo_bin_cmd!()
            .write_stdin(url)
            .arg("--max-redirects")
            .arg("0")
            .arg("-")
            .assert()
            .failure();
    }

    #[test]
    #[ignore = "Skipping test because it is flaky"]
    fn test_suggests_url_alternatives() -> Result<()> {
        let re = Regex::new(r"http://web\.archive\.org/web/.*google\.com/jobs\.html").unwrap();

        for _ in 0..3 {
            // This can be flaky. Try up to 3 times
            let mut cmd = cargo_bin_cmd!();
            let input = fixtures_path!().join("INTERNET_ARCHIVE.md");

            cmd.arg("--no-progress").arg("--suggest").arg(input);

            // Run he command and check if the output contains the expected
            // suggestions
            let assert = cmd.assert();
            let output = assert.get_output();

            // We're looking for a suggestion that
            // - starts with http://web.archive.org/web/
            // - ends with google.com/jobs.html
            if re.is_match(&String::from_utf8_lossy(&output.stdout)) {
                // Test passed
                return Ok(());
            } else {
                // Wait for a second before retrying
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }

        // If we reached here, it means the test did not pass after multiple attempts
        Err("Did not get the expected command output after multiple attempts.".into())
    }

    #[tokio::test]
    async fn test_basic_auth() {
        let username = "username";
        let password = "password123";

        let mock_server = wiremock::MockServer::start().await;

        Mock::given(method("GET"))
            .and(basic_auth(username, password))
            .respond_with(ResponseTemplate::new(200)) // Authenticated requests are accepted
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .respond_with(|_: &_| panic!("Received unauthenticated request"))
            .mount(&mock_server)
            .await;

        // Configure the command to use the BasicAuthExtractor
        cargo_bin_cmd!()
            .arg("--verbose")
            .arg("--basic-auth")
            .arg(format!("{} {username}:{password}", mock_server.uri()))
            .arg("-")
            .write_stdin(mock_server.uri())
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 OK"));

        // Websites as direct arguments must also use authentication
        cargo_bin_cmd!()
            .arg(mock_server.uri())
            .arg("--verbose")
            .arg("--basic-auth")
            .arg(format!("{} {username}:{password}", mock_server.uri()))
            .assert()
            .success()
            .stdout(contains("0 Total")); // Mock server returns no body, so there are no URLs to check
    }

    #[tokio::test]
    async fn test_multi_basic_auth() {
        let username1 = "username";
        let password1 = "password123";
        let mock_server1 = wiremock::MockServer::start().await;
        Mock::given(basic_auth(username1, password1))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server1)
            .await;

        let username2 = "admin_user";
        let password2 = "admin_pw";
        let mock_server2 = wiremock::MockServer::start().await;

        Mock::given(basic_auth(username2, password2))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server2)
            .await;

        // Configure the command to use the BasicAuthExtractor
        cargo_bin_cmd!()
            .arg("--verbose")
            .arg("--basic-auth")
            .arg(format!("{} {username1}:{password1}", mock_server1.uri()))
            .arg("--basic-auth")
            .arg(format!("{} {username2}:{password2}", mock_server2.uri()))
            .arg("-")
            .write_stdin(format!("{}\n{}", mock_server1.uri(), mock_server2.uri()))
            .assert()
            .success()
            .stdout(contains("2 Total"))
            .stdout(contains("2 OK"));
    }

    #[tokio::test]
    async fn test_cookie_jar() -> Result<()> {
        // Create a random cookie jar file
        let cookie_jar = NamedTempFile::new()?;
        cargo_bin_cmd!()
            .arg("--cookie-jar")
            .arg(cookie_jar.path().to_str().unwrap())
            .arg("-")
            // Using Google as a test target because I couldn't
            // get the mock server to work with the cookie jar
            .write_stdin("https://google.com/ncr")
            .assert()
            .success();

        // check that the cookie jar file contains the expected cookies
        let file = std::fs::File::open(cookie_jar.path()).map(std::io::BufReader::new)?;
        let cookie_store = cookie_store::serde::json::load(file)
            .map_err(|e| anyhow!("Failed to load cookie jar: {e}"))?;
        let all_cookies = cookie_store.iter_any().collect::<Vec<_>>();
        assert!(!all_cookies.is_empty());
        assert!(all_cookies.iter().all(|c| c.domain() == Some("google.com")));
        Ok(())
    }

    #[test]
    fn test_dump_inputs_does_not_include_duplicates() {
        let pattern = fixtures_path!().join("dump_inputs/markdown.md");

        cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg(&pattern)
            .arg(&pattern)
            .assert()
            .success()
            .stdout(contains("fixtures/dump_inputs/markdown.md").count(1));
    }

    #[test]
    fn test_dump_inputs_glob_does_not_include_duplicates() {
        let pattern1 = fixtures_path!().join("**/markdown.*");
        let pattern2 = fixtures_path!().join("**/*.md");

        cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg(pattern1)
            .arg(pattern2)
            .assert()
            .success()
            .stdout(contains("fixtures/dump_inputs/markdown.md").count(1));
    }

    #[test]
    fn test_dump_inputs_glob_md() {
        let pattern = fixtures_path!().join("**/*.md");

        cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg(pattern)
            .assert()
            .success()
            .stdout(contains("fixtures/dump_inputs/subfolder/file2.md"))
            .stdout(contains("fixtures/dump_inputs/markdown.md"));
    }

    #[test]
    fn test_dump_inputs_glob_all() {
        let pattern = fixtures_path!().join("**/*");

        cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg(pattern)
            .assert()
            .success()
            .stdout(contains("fixtures/dump_inputs/subfolder/test.html"))
            .stdout(contains("fixtures/dump_inputs/subfolder/file2.md"))
            .stdout(contains("fixtures/dump_inputs/subfolder"))
            .stdout(contains("fixtures/dump_inputs/markdown.md"))
            .stdout(contains("fixtures/dump_inputs/some_file.txt"));
    }

    #[test]
    fn test_dump_inputs_glob_exclude_path() {
        let pattern = fixtures_path!().join("**/*");

        cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg(pattern)
            .arg("--exclude-path")
            .arg(fixtures_path!().join("dump_inputs/subfolder"))
            .assert()
            .success()
            .stdout(contains("fixtures/dump_inputs/subfolder/test.html").not())
            .stdout(contains("fixtures/dump_inputs/subfolder/file2.md").not())
            .stdout(contains("fixtures/dump_inputs/subfolder").not());
    }

    #[test]
    fn test_dump_inputs_url() {
        let result = cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg("https://example.com")
            .assert()
            .success();

        assert_lines_eq(result, vec!["https://example.com/"]);
    }

    #[test]
    fn test_dump_inputs_path() {
        let result = cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg(fixtures_path!().join("dump_inputs"))
            .assert()
            .success();

        let base_path = fixtures_path!().join("dump_inputs");
        let expected_lines = [
            "some_file.txt",
            "subfolder/file2.md",
            "subfolder/test.html",
            "markdown.md",
        ]
        .iter()
        .map(|p| path_str(&base_path, p))
        .collect();

        assert_lines_eq(result, expected_lines);
    }

    // Ensures that dumping stdin does not panic and results in an empty output
    // as `stdin` is not a path
    #[test]
    fn test_dump_inputs_with_extensions() {
        let test_dir = fixtures_path!().join("dump_inputs");

        let output = cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg("--extensions")
            .arg("md,txt")
            .arg(test_dir)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let mut actual_lines: Vec<String> = output
            .lines()
            .map(|line| line.unwrap().to_string())
            .collect();
        actual_lines.sort();

        let base_path = fixtures_path!().join("dump_inputs");
        let mut expected_lines = vec![
            path_str(&base_path, "some_file.txt"),
            path_str(&base_path, "subfolder/file2.md"),
            path_str(&base_path, "markdown.md"),
        ];
        expected_lines.sort();

        assert_eq!(actual_lines, expected_lines);

        // Verify example.bin is not included
        for line in &actual_lines {
            assert!(
                !line.contains("example.bin"),
                "Should not contain example.bin: {line}"
            );
        }
    }

    #[test]
    fn test_dump_inputs_skip_hidden() {
        let test_dir = fixtures_path!().join("hidden");

        // Test default behavior (skip hidden)
        cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg(&test_dir)
            .assert()
            .success()
            .stdout(is_empty());

        // Test with --hidden flag
        cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg("--hidden")
            .arg(test_dir)
            .assert()
            .success()
            .stdout(contains("hidden/.file.md"))
            .stdout(contains("hidden/.hidden/file.md"));
    }

    #[test]
    fn test_dump_inputs_individual_file() {
        let test_file = fixtures_path!().join("TEST.md");

        cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg(&test_file)
            .assert()
            .success()
            .stdout(contains("fixtures/TEST.md"));
    }

    #[test]
    fn test_dump_inputs_stdin() {
        cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg("-")
            .assert()
            .success()
            .stdout(contains("<stdin>"));
    }

    #[test]
    fn test_fragments_regression() {
        let input = fixtures_path!().join("FRAGMENT_REGRESSION.md");

        cargo_bin_cmd!()
            .arg("--include-fragments")
            .arg("--verbose")
            .arg(input)
            .assert()
            .failure();
    }

    #[test]
    fn test_fragments() {
        let input = fixtures_path!().join("fragments");

        let mut result = cargo_bin_cmd!()
            .arg("--include-fragments")
            .arg("--verbose")
            .arg(input)
            .assert()
            .failure();

        let expected_successes = vec![
            "fixtures/fragments/empty_dir",
            "fixtures/fragments/empty_file#fragment", // XXX: is this a bug? a fragment in an empty file is being treated as valid
            "fixtures/fragments/file1.md#code-heading",
            "fixtures/fragments/file1.md#explicit-fragment",
            "fixtures/fragments/file1.md#f%C3%BCnf-s%C3%9C%C3%9Fe-%C3%84pfel",
            "fixtures/fragments/file1.md#f%C3%BCnf-s%C3%BC%C3%9Fe-%C3%A4pfel",
            "fixtures/fragments/file1.md#fragment-1",
            "fixtures/fragments/file1.md#fragment-2",
            "fixtures/fragments/file1.md#IGNORE-CASING",
            "fixtures/fragments/file1.md#kebab-case-fragment",
            "fixtures/fragments/file1.md#kebab-case-fragment-1",
            "fixtures/fragments/file1.md#lets-wear-a-hat-%C3%AAtre",
            "fixtures/fragments/file2.md#",
            "fixtures/fragments/file2.md#custom-id",
            "fixtures/fragments/file2.md#fragment-1",
            "fixtures/fragments/file2.md#top",
            "fixtures/fragments/file.html#",
            "fixtures/fragments/file.html#a-word",
            "fixtures/fragments/file.html#in-the-beginning",
            "fixtures/fragments/file.html#tangent%3A-kustomize",
            "fixtures/fragments/file.html#top",
            "fixtures/fragments/file.html#Upper-%C3%84%C3%96%C3%B6",
            "fixtures/fragments/sub_dir",
            "fixtures/fragments/zero.bin",
            "fixtures/fragments/zero.bin#",
            "fixtures/fragments/zero.bin#fragment",
            "https://github.com/lycheeverse/lychee#table-of-contents",
            "https://raw.githubusercontent.com/lycheeverse/lychee/master/fixtures/fragments/zero.bin",
            "https://raw.githubusercontent.com/lycheeverse/lychee/master/fixtures/fragments/zero.bin#",
            // zero.bin#fragment succeeds because fragment checking is skipped for this URL
            "https://raw.githubusercontent.com/lycheeverse/lychee/master/fixtures/fragments/zero.bin#fragment",
        ];

        let expected_failures = vec![
            "fixtures/fragments/sub_dir_non_existing_1",
            "fixtures/fragments/sub_dir#non-existing-fragment-2",
            "fixtures/fragments/sub_dir#a-link-inside-index-html-inside-sub-dir",
            "fixtures/fragments/empty_dir#non-existing-fragment-3",
            "fixtures/fragments/file2.md#missing-fragment",
            "fixtures/fragments/sub_dir#non-existing-fragment-1",
            "fixtures/fragments/sub_dir_non_existing_2",
            "fixtures/fragments/file1.md#missing-fragment",
            "fixtures/fragments/empty_dir#non-existing-fragment-4",
            "fixtures/fragments/file.html#in-the-end",
            "fixtures/fragments/file.html#in-THE-begiNNing",
            "https://github.com/lycheeverse/lychee#non-existent-anchor",
        ];

        // the stdout/stderr format looks like this:
        //
        //     [ERROR] https://github.com/lycheeverse/lychee#non-existent-anchor | Cannot find fragment
        //     [200] file:///home/rina/progs/lychee/fixtures/fragments/file.html#a-word
        //
        // errors are printed to both, but 200s are printed to stderr only.
        // we take advantage of this to ensure that good URLs do not appear
        // in stdout, and bad URLs do appear in stdout.
        //
        // also, a space or newline is appended to the URL to prevent
        // incorrect matches where one URL is a prefix of another.
        for good_url in &expected_successes {
            // additionally checks that URL is within stderr to ensure that
            // the URL is detected by lychee.
            result = result
                .stdout(contains(format!("{good_url} ")).not())
                .stderr(contains(format!("{good_url}\n")));
        }
        for bad_url in &expected_failures {
            result = result.stdout(contains(format!("{bad_url} ")));
        }

        let ok_num = expected_successes.len();
        let err_num = expected_failures.len();
        let total_num = ok_num + err_num;
        result
            .stdout(contains(format!("{ok_num} OK")))
            // Failures because of missing fragments or failed binary body scan
            .stdout(contains(format!("{err_num} Errors")))
            .stdout(contains(format!("{total_num} Total")));
    }

    #[test]
    fn test_fragments_when_accept_error_status_codes() {
        let input = fixtures_path!().join("TEST_FRAGMENT_ERR_CODE.md");

        // it's common for user to accept 429, but let's test with 404 since
        // triggering 429 may annoy the server
        cargo_bin_cmd!()
            .arg("--verbose")
            .arg("--accept=200,404")
            .arg("--include-fragments")
            .arg(input)
            .assert()
            .success()
            .stderr(contains(
                "https://en.wikipedia.org/wiki/Should404#ignore-fragment",
            ))
            .stdout(contains("0 Errors"))
            .stdout(contains("1 OK"))
            .stdout(contains("1 Total"));
    }

    #[test]
    fn test_fallback_extensions() {
        let input = fixtures_path!().join("fallback-extensions");

        cargo_bin_cmd!()
            .arg("--verbose")
            .arg("--fallback-extensions=htm,html")
            .arg(input)
            .assert()
            .success()
            .stdout(contains("0 Errors"));
    }

    #[test]
    fn test_fragments_fallback_extensions() {
        let input = fixtures_path!().join("fragments-fallback-extensions");

        cargo_bin_cmd!()
            .arg("--include-fragments")
            .arg("--fallback-extensions=html")
            .arg("--no-progress")
            .arg("--offline")
            .arg("-v")
            .arg(input)
            .assert()
            .failure()
            .stdout(contains("3 Total"))
            .stdout(contains("1 OK"))
            .stdout(contains("2 Errors"));
    }

    /// Test relative paths
    ///
    /// Imagine a web server hosting a site with the following structure:
    /// root
    /// └── test
    ///     ├── index.html
    ///     └── next.html
    ///
    /// where `root/test/index.html` contains `<a href="next.html">next</a>`
    /// When checking the link in `root/test/index.html` we should be able to
    /// resolve the relative path to `root/test/next.html`
    ///
    /// Note that the relative path is not resolved to the root of the server
    /// but relative to the file that contains the link.
    #[tokio::test]
    async fn test_resolve_relative_paths_in_subfolder() {
        let mock_server = wiremock::MockServer::start().await;

        let body = r#"<a href="next.html">next</a>"#;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/test/index.html"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(body))
            .mount(&mock_server)
            .await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/test/next.html"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        cargo_bin_cmd!()
            .arg("--verbose")
            .arg(format!("{}/test/index.html", mock_server.uri()))
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("0 Errors"));
    }

    #[tokio::test]
    async fn test_json_format_in_config() -> Result<()> {
        let mock_server = mock_server!(StatusCode::OK);
        let config = fixtures_path!().join("configs").join("format.toml");
        let output = cargo_bin_cmd!()
            .arg("--config")
            .arg(config)
            .arg("-")
            .write_stdin(mock_server.uri())
            .env_clear()
            .assert()
            .success()
            .get_output()
            .clone();

        // Check that the output is in JSON format
        let output = std::str::from_utf8(&output.stdout)?;
        let json: serde_json::Value = serde_json::from_str(output)?;
        assert_eq!(json["total"], 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_redirect_json() {
        use serde_json::json;
        redirecting_mock_server!(async |redirect_url: Url, ok_url| {
            let output = cargo_bin_cmd!()
                .arg("-")
                .arg("--format")
                .arg("json")
                .write_stdin(redirect_url.as_str())
                .env_clear()
                .assert()
                .success()
                .get_output()
                .clone()
                .unwrap();

            // Check that the output is in JSON format
            let output = std::str::from_utf8(&output.stdout).unwrap();
            let json: serde_json::Value = serde_json::from_str(output).unwrap();
            assert_eq!(json["total"], 1);
            assert_eq!(json["redirects"], 1);
            assert_eq!(
                json["redirect_map"],
                json!({
                "stdin":[{
                    "status": {
                        "code": 200,
                        "text": "Redirect",
                        "redirects": [ redirect_url, ok_url ]
                    },
                    "url": redirect_url
                }]})
            );
        })
        .await;
    }

    #[tokio::test]
    async fn test_retry() {
        let mock_server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        cargo_bin_cmd!()
            .arg("-")
            .write_stdin(mock_server.uri())
            .assert()
            .success();
    }

    #[tokio::test]
    async fn test_retry_rate_limit_headers() {
        const RETRY_DELAY: Duration = Duration::from_secs(1);
        const TOLERANCE: Duration = Duration::from_millis(500);
        let server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(
                ResponseTemplate::new(429)
                    .append_header("Retry-After", RETRY_DELAY.as_secs().to_string()),
            )
            .expect(1)
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let start = Instant::now();
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(move |_: &Request| {
                let delta = Instant::now().duration_since(start);
                assert!(delta > RETRY_DELAY);
                assert!(delta < RETRY_DELAY + TOLERANCE);
                ResponseTemplate::new(200)
            })
            .expect(1)
            .mount(&server)
            .await;

        cargo_bin_cmd!()
            // Direct args are not using the host pool, they are resolved earlier via Collector
            .arg("-")
            // Retry wait times are added on top of host-specific backoff timeout
            .arg("--retry-wait-time")
            .arg("0")
            .write_stdin(server.uri())
            .assert()
            .success();

        // Check that the server received the request with the header
        server.verify().await;
    }

    #[tokio::test]
    async fn test_no_header_set_on_input() {
        let server = wiremock::MockServer::start().await;
        server
            .register(
                wiremock::Mock::given(wiremock::matchers::method("GET"))
                    .respond_with(wiremock::ResponseTemplate::new(200))
                    .expect(1),
            )
            .await;

        cargo_bin_cmd!()
            .arg("--verbose")
            .arg(server.uri())
            .assert()
            .success();

        let received_requests = server.received_requests().await.unwrap();
        assert_eq!(received_requests.len(), 1);

        let received_request = &received_requests[0];
        assert_eq!(received_request.method, Method::GET);
        assert_eq!(received_request.url.path(), "/");

        // Make sure the request does not contain the custom header
        assert!(!received_request.headers.contains_key("X-Foo"));
    }

    #[tokio::test]
    async fn test_header_set_on_input() {
        let server = wiremock::MockServer::start().await;
        server
            .register(
                wiremock::Mock::given(wiremock::matchers::method("GET"))
                    .and(wiremock::matchers::header("X-Foo", "Bar"))
                    .respond_with(wiremock::ResponseTemplate::new(200))
                    .expect(1)
                    .named("GET expecting custom header"),
            )
            .await;

        cargo_bin_cmd!()
            .arg("--verbose")
            .arg("--header")
            .arg("X-Foo: Bar")
            .arg(server.uri())
            .assert()
            .success();

        // Check that the server received the request with the header
        server.verify().await;
    }

    #[tokio::test]
    async fn test_multi_header_set_on_input() {
        let server = wiremock::MockServer::start().await;
        server
            .register(
                wiremock::Mock::given(wiremock::matchers::method("GET"))
                    .and(wiremock::matchers::header("X-Foo", "Bar"))
                    .and(wiremock::matchers::header("X-Bar", "Baz"))
                    .respond_with(wiremock::ResponseTemplate::new(200))
                    .expect(1)
                    .named("GET expecting custom header"),
            )
            .await;

        cargo_bin_cmd!()
            .arg("--verbose")
            .arg("--header")
            .arg("X-Foo: Bar")
            .arg("--header")
            .arg("X-Bar: Baz")
            .arg(server.uri())
            .assert()
            .success();

        // Check that the server received the request with the header
        server.verify().await;
    }

    #[tokio::test]
    async fn test_header_set_in_config() {
        let server = wiremock::MockServer::start().await;
        server
            .register(
                wiremock::Mock::given(wiremock::matchers::method("GET"))
                    .and(wiremock::matchers::header("X-Foo", "Bar"))
                    .and(wiremock::matchers::header("X-Bar", "Baz"))
                    .and(wiremock::matchers::header("X-Host-Specific", "Foo"))
                    .respond_with(wiremock::ResponseTemplate::new(200))
                    .expect(1)
                    .named("GET expecting custom header"),
            )
            .await;

        let config = fixtures_path!().join("configs").join("headers.toml");
        cargo_bin_cmd!()
            .arg("--verbose")
            .arg("--config")
            .arg(config)
            .arg("-")
            .write_stdin(server.uri())
            .assert()
            .success();

        // Check that the server received the request with the header
        server.verify().await;
    }

    #[test]
    fn test_sorted_error_output() {
        let test_files = ["TEST_GITHUB_404.md", "TEST_INVALID_URLS.html"];
        let test_urls = [
            "https://httpbin.org/status/404",
            "https://httpbin.org/status/500",
            "https://httpbin.org/status/502",
        ];

        let cmd = &mut cargo_bin_cmd!()
            .arg("--format")
            .arg("compact")
            .arg(fixtures_path!().join(test_files[1]))
            .arg(fixtures_path!().join(test_files[0]))
            .assert()
            .failure()
            .code(2);

        let output = String::from_utf8_lossy(&cmd.get_output().stdout);
        let mut position: usize = 0;

        // Check that the input sources are sorted
        for file in test_files {
            assert!(output.contains(file));

            let next_position = output.find(file).unwrap();

            assert!(next_position > position);
            position = next_position;
        }

        position = 0;

        // Check that the responses are sorted
        for url in test_urls {
            assert!(output.contains(url));

            let next_position = output.find(url).unwrap();

            assert!(next_position > position);
            position = next_position;
        }
    }

    #[test]
    fn test_extract_url_ending_with_period_file() {
        let test_path = fixtures_path!().join("LINK_PERIOD.html");

        cargo_bin_cmd!()
            .arg("--dump")
            .arg(test_path)
            .assert()
            .success()
            .stdout(contains("https://www.example.com/smth."));
    }

    #[tokio::test]
    async fn test_extract_url_ending_with_period_webserver() {
        let body = r#"<a href="https://www.example.com/smth.">link</a>"#;
        let mock_server = mock_response!(body);

        cargo_bin_cmd!()
            .arg("--dump")
            .arg(mock_server.uri())
            .assert()
            .success()
            .stdout(contains("https://www.example.com/smth."));
    }

    #[test]
    fn test_wikilink_extract_when_specified() {
        let test_path = fixtures_path!().join("TEST_WIKI.md");

        cargo_bin_cmd!()
            .arg("--dump")
            .arg("--include-wikilinks")
            .arg("--base-url")
            .arg(fixtures_path!())
            .arg(test_path)
            .assert()
            .success()
            .stdout(contains("LycheeWikilink"));
    }

    #[test]
    fn test_wikilink_dont_extract_when_not_specified() {
        let test_path = fixtures_path!().join("TEST_WIKI.md");

        cargo_bin_cmd!()
            .arg("--dump")
            .arg(test_path)
            .assert()
            .success()
            .stdout(is_empty());
    }

    #[test]
    fn test_index_files_default() {
        let input = fixtures_path!().join("filechecker/dir_links.md");

        // the dir links in this file all exist.
        cargo_bin_cmd!()
            .arg(&input)
            .arg("--verbose")
            .assert()
            .success();

        // ... but checking fragments will find none, because dirs
        // have no fragments and no index file given.
        let dir_links_with_fragment = 2;
        cargo_bin_cmd!()
            .arg(&input)
            .arg("--include-fragments")
            .assert()
            .failure()
            .stdout(contains("Cannot find fragment").count(dir_links_with_fragment))
            .stdout(contains("#").count(dir_links_with_fragment));
    }

    #[test]
    fn test_index_files_specified() {
        let input = fixtures_path!().join("filechecker/dir_links.md");

        // passing `--index-files index.html,index.htm` should reject all links
        // to /empty_dir because it doesn't have the index file
        let result = cargo_bin_cmd!()
            .arg(&input)
            .arg("--index-files")
            .arg("index.html,index.htm")
            .arg("--verbose")
            .assert()
            .failure();

        let empty_dir_links = 2;
        let index_dir_links = 2;
        result
            .stdout(contains("Cannot find index file").count(empty_dir_links))
            .stdout(contains("/empty_dir").count(empty_dir_links))
            .stdout(contains("(index.html, or index.htm)").count(empty_dir_links))
            .stdout(contains(format!("{index_dir_links} OK")));

        // within the error message, formatting of the index file name list should
        // omit empty names.
        cargo_bin_cmd!()
            .arg(&input)
            .arg("--index-files")
            .arg(",index.html,,,index.htm,")
            .assert()
            .failure()
            .stdout(contains("(index.html, or index.htm)").count(empty_dir_links));
    }

    #[test]
    fn test_index_files_dot_in_list() {
        let input = fixtures_path!().join("filechecker/dir_links.md");

        // passing `.` in the index files list should accept a directory
        // even if no other index file is found.
        cargo_bin_cmd!()
            .arg(&input)
            .arg("--index-files")
            .arg("index.html,.")
            .assert()
            .success()
            .stdout(contains("4 OK"));

        // checking fragments will accept the index_dir#fragment link,
        // but reject empty_dir#fragment because empty_dir doesn’t have
        // index.html.
        cargo_bin_cmd!()
            .arg(&input)
            .arg("--index-files")
            .arg("index.html,.")
            .arg("--include-fragments")
            .assert()
            .failure()
            .stdout(contains("Cannot find fragment").count(1))
            .stdout(contains("empty_dir#fragment").count(1))
            .stdout(contains("index_dir#fragment").count(0))
            .stdout(contains("3 OK"));
    }

    #[test]
    fn test_index_files_empty_list() {
        let input = fixtures_path!().join("filechecker/dir_links.md");

        // passing an empty list to --index-files should reject /all/
        // directory links.
        let result = cargo_bin_cmd!()
            .arg(&input)
            .arg("--index-files")
            .arg("")
            .assert()
            .failure();

        let num_dir_links = 4;
        result
            .stdout(contains("Cannot find index file").count(num_dir_links))
            .stdout(contains("No directory links are allowed").count(num_dir_links))
            .stdout(contains("0 OK"));

        // ... as should passing a number of empty index file names
        cargo_bin_cmd!()
            .arg(&input)
            .arg("--index-files")
            .arg(",,,,,")
            .assert()
            .failure()
            .stdout(contains("No directory links are allowed").count(num_dir_links))
            .stdout(contains("0 OK"));
    }

    #[test]
    fn test_skip_binary_input() {
        // A path containing a binary file
        let inputs = fixtures_path!().join("invalid_utf8");

        // Run the command with the binary input
        let result = cargo_bin_cmd!()
            .arg("--verbose")
            .arg(&inputs)
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 OK"))
            .stdout(contains("0 Errors"));

        result
            .stderr(contains(format!(
                "Skipping file with invalid UTF-8 content: {}",
                inputs.join("invalid_utf8.txt").display()
            )))
            .stderr(contains("https://example.com/"));
    }

    /// Checks that the `--dump-inputs` command does not panic
    /// when given a path that contains invalid UTF-8 characters.
    ///
    /// The command should still succeed and output the paths of the files it
    /// found, including those with invalid UTF-8, which will be skipped during
    /// processing.
    #[test]
    fn test_dump_invalid_utf8_inputs() {
        // A path containing a binary file
        let inputs = fixtures_path!().join("invalid_utf8");

        // Run the command with the binary input
        cargo_bin_cmd!()
            .arg("--dump-inputs")
            .arg(inputs)
            .assert()
            .success()
            .stdout(contains("fixtures/invalid_utf8/index.html"))
            .stdout(contains("fixtures/invalid_utf8/invalid_utf8.txt"));
    }

    /// Check that files specified via glob patterns are always checked
    /// no matter their extension. I.e. extensions are ignored for files
    /// explicitly specified by the user.
    ///
    /// See https://github.com/lycheeverse/lychee-action/issues/305
    #[test]
    fn test_globbed_files_are_always_checked() {
        let input = fixtures_path!().join("glob_dir/**/*.tsx");

        // The directory contains:
        // - example.ts
        // - example.tsx
        // - example.md
        // - example.html
        // But the user only specified the .tsx file via the glob pattern.
        cargo_bin_cmd!()
            .arg("--verbose")
            // Only check ts, js, and html files by default.
            // However, all files explicitly specified by the user
            // should always be checked so this should be ignored.
            .arg("--extensions=ts,js,html")
            .arg(input)
            .assert()
            .failure()
            .stdout(contains("1 Total"))
            .stderr(contains("https://example.com/glob_dir/tsx"));
    }

    #[test]
    fn test_extensions_work_on_glob_files_directory() {
        let input = fixtures_path!().join("glob_dir");

        // Make sure all files matching the given extensions are checked
        // if we specify a directory (and not a glob pattern).
        cargo_bin_cmd!()
            .arg("--verbose")
            .arg("--extensions=ts,html")
            .arg(input)
            .assert()
            .failure()
            .stdout(contains("2 Total"))
            // Note: The space is intentional to avoid matching tsx.
            .stderr(contains("https://example.com/glob_dir/ts "))
            // TSX files are ignored because we did not specify
            // that extension. So `https://example.com/tsx"` should be missing from the output.
            .stderr(contains("https://example.com/glob_dir/tsx").not())
            // Markdown is also ignored because we did not specify that extension.
            .stderr(contains("https://example.com/glob_dir/md").not())
            .stderr(contains("https://example.com/glob_dir/html"));
    }

    /// We define two inputs, one being a glob pattern, the other a directory path.
    /// The extensions should only apply to the directory path, not the glob pattern.
    #[test]
    fn test_extensions_apply_to_files_not_globs() {
        let glob_input = fixtures_path!().join("glob_dir/**/*.tsx");
        let dir_input = fixtures_path!().join("example_dir");

        cargo_bin_cmd!()
            .arg("--verbose")
            .arg("--extensions=html,md")
            .arg(glob_input)
            .arg(dir_input)
            .assert()
            .failure()
            .stdout(contains("3 Total"))
            // Only TSX files are matched by the glob pattern.
            .stderr(contains("https://example.com/glob_dir/tsx"))
            .stderr(contains("https://example.com/glob_dir/ts ").not())
            .stderr(contains("https://example.com/glob_dir/md").not())
            .stderr(contains("https://example.com/glob_dir/html").not())
            // For the example_dir, the extensions should apply.
            .stderr(contains("https://example.com/example_dir/html"))
            .stderr(contains("https://example.com/example_dir/md"))
            // TS files in example_dir are ignored because we did not specify that extension.
            .stderr(contains("https://example.com/example_dir/ts ").not())
            // TSX files in example_dir are ignored because we did not specify that extension.
            .stderr(contains("https://example.com/example_dir/tsx").not());
    }

    /// Individual files should always be checked, even if their
    /// extension does not match the given extensions.
    #[test]
    fn test_file_inputs_always_get_checked_no_matter_their_extension() {
        let ts_input_file = fixtures_path!().join("glob_dir/example.ts");
        let md_input_file = fixtures_path!().join("glob_dir/example.md");

        cargo_bin_cmd!()
            .arg("--verbose")
            .arg("--dump")
            .arg("--extensions=html,md")
            .arg(ts_input_file)
            .arg(md_input_file)
            .assert()
            .success()
            .stderr("") // Ensure stderr is empty
            .stdout(contains("https://example.com/glob_dir/ts"))
            .stdout(contains("https://example.com/glob_dir/md"));
    }

    /// URLs specified on the command line should also always be checked.
    /// For example, sitemap URLs often end with `.xml` which is not
    /// a file extension we would check by default.
    #[test]
    fn test_url_inputs_always_get_checked_no_matter_their_extension() {
        let url_input = "https://example.com/sitemap.xml";

        cargo_bin_cmd!()
            .arg("--verbose")
            .arg("--dump")
            .arg(url_input)
            .assert()
            .success()
            .stderr("") // Ensure stderr is empty
            .stdout(contains("https://example.com/sitemap.xml"));
    }

    #[test]
    fn test_files_from_file() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let files_list_path = temp_dir.path().join("files.txt");
        let test_md = temp_dir.path().join("test.md");

        // Create test files
        fs::write(&test_md, "# Test\n[link](https://example.com)")?;
        fs::write(&files_list_path, test_md.to_string_lossy().as_ref())?;

        cargo_bin_cmd!()
            .arg("--files-from")
            .arg(&files_list_path)
            .arg("--dump-inputs")
            .assert()
            .success()
            .stdout(contains(test_md.to_string_lossy().as_ref()));

        Ok(())
    }

    #[test]
    fn test_files_from_stdin() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_md = temp_dir.path().join("test.md");

        // Create test file
        fs::write(&test_md, "# Test\n[link](https://example.com)")?;

        cargo_bin_cmd!()
            .arg("--files-from")
            .arg("-")
            .arg("--dump-inputs")
            .write_stdin(test_md.to_string_lossy().as_ref())
            .assert()
            .success()
            .stdout(contains(test_md.to_string_lossy().as_ref()));

        Ok(())
    }

    #[test]
    fn test_files_from_with_comments_and_empty_lines() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let files_list_path = temp_dir.path().join("files.txt");
        let test_md = temp_dir.path().join("test.md");

        // Create test files
        fs::write(&test_md, "# Test\n[link](https://example.com)")?;
        fs::write(
            &files_list_path,
            format!(
                "# Comment line\n\n{}\n# Another comment\n",
                test_md.display()
            ),
        )?;

        cargo_bin_cmd!()
            .arg("--files-from")
            .arg(&files_list_path)
            .arg("--dump-inputs")
            .assert()
            .success()
            .stdout(contains(test_md.to_string_lossy().as_ref()));

        Ok(())
    }

    #[test]
    fn test_files_from_combined_with_regular_inputs() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let files_list_path = temp_dir.path().join("files.txt");
        let test_md1 = temp_dir.path().join("test1.md");
        let test_md2 = temp_dir.path().join("test2.md");

        // Create test files
        fs::write(&test_md1, "# Test 1")?;
        fs::write(&test_md2, "# Test 2")?;
        fs::write(&files_list_path, test_md1.to_string_lossy().as_ref())?;

        let mut cmd = cargo_bin_cmd!();
        cmd.arg("--files-from")
            .arg(&files_list_path)
            .arg(&test_md2) // Regular input argument
            .arg("--dump-inputs")
            .assert()
            .success()
            .stdout(contains(test_md1.to_string_lossy().as_ref()))
            .stdout(contains(test_md2.to_string_lossy().as_ref()));

        Ok(())
    }

    #[test]
    fn test_files_from_nonexistent_file_error() {
        cargo_bin_cmd!()
            .arg("--files-from")
            .arg("/nonexistent/file.txt")
            .arg("--dump-inputs")
            .assert()
            .failure()
            .stderr(contains("Cannot open --files-from file"));
    }

    /// Test the --default-extension option for files without extensions
    #[test]
    fn test_default_extension_option() -> Result<()> {
        let mut file_without_ext = NamedTempFile::new()?;
        // Create markdown content but with no file extension
        writeln!(file_without_ext, "# Test File")?;
        writeln!(file_without_ext, "[Example](https://example.com)")?;
        writeln!(file_without_ext, "[Local](local.md)")?;

        // Test with --default-extension md
        cargo_bin_cmd!()
            .arg("--default-extension")
            .arg("md")
            .arg("--dump")
            .arg(file_without_ext.path())
            .assert()
            .success()
            .stdout(contains("https://example.com"));

        let mut html_file_without_ext = NamedTempFile::new()?;
        // Create HTML content but with no file extension
        writeln!(html_file_without_ext, "<html><body>")?;
        writeln!(
            html_file_without_ext,
            "<a href=\"https://html-example.com\">HTML Link</a>"
        )?;
        writeln!(html_file_without_ext, "</body></html>")?;

        // Test with --default-extension html
        cargo_bin_cmd!()
            .arg("--default-extension")
            .arg("html")
            .arg("--dump")
            .arg(html_file_without_ext.path())
            .assert()
            .success()
            .stdout(contains("https://html-example.com"));

        Ok(())
    }

    /// Test that unknown --default-extension values are handled gracefully
    #[test]
    fn test_default_extension_unknown_value() {
        let mut file_without_ext = NamedTempFile::new().unwrap();
        // Create file content with a link that should be extracted as plaintext
        writeln!(file_without_ext, "# Test").unwrap();
        writeln!(file_without_ext, "Visit https://example.org for more info").unwrap();

        // Unknown extensions should fall back to default behavior (plaintext)
        // and still extract links from the content
        cargo_bin_cmd!()
            .arg("--default-extension")
            .arg("unknown")
            .arg("--dump")
            .arg(file_without_ext.path())
            .assert()
            .success()
            .stdout(contains("https://example.org")); // Should extract the link as plaintext
    }

    #[test]
    fn test_wikilink_fixture_obsidian_style() {
        let input = fixtures_path!().join("wiki/obsidian-style.md");

        // testing without fragments should not yield failures
        cargo_bin_cmd!()
            .arg(&input)
            .arg("--include-wikilinks")
            .arg("--fallback-extensions")
            .arg("md")
            .arg("--base-url")
            .arg(fixtures_path!())
            .assert()
            .success()
            .stdout(contains("4 OK"));
    }

    #[test]
    fn test_wikilink_fixture_wikilink_non_existent() {
        let input = fixtures_path!().join("wiki/Non-existent.md");

        cargo_bin_cmd!()
            .arg(&input)
            .arg("--include-wikilinks")
            .arg("--fallback-extensions")
            .arg("md")
            .arg("--base-url")
            .arg(fixtures_path!())
            .assert()
            .failure()
            .stdout(contains("3 Errors"));
    }

    #[test]
    fn test_wikilink_fixture_with_fragments_obsidian_style_fixtures_excluded() {
        let input = fixtures_path!().join("wiki/obsidian-style-plus-headers.md");

        // fragments should resolve all headers
        cargo_bin_cmd!()
            .arg(&input)
            .arg("--include-wikilinks")
            .arg("--fallback-extensions")
            .arg("md")
            .arg("--base-url")
            .arg(fixtures_path!())
            .assert()
            .success()
            .stdout(contains("4 OK"));
    }

    #[test]
    fn test_wikilink_fixture_with_fragments_obsidian_style() {
        let input = fixtures_path!().join("wiki/obsidian-style-plus-headers.md");

        // fragments should resolve all headers
        cargo_bin_cmd!()
            .arg(&input)
            .arg("--include-wikilinks")
            .arg("--include-fragments")
            .arg("--fallback-extensions")
            .arg("md")
            .arg("--base-url")
            .arg(fixtures_path!())
            .assert()
            .success()
            .stdout(contains("4 OK"));
    }

    /// An input which matches nothing should print a warning and continue.
    #[test]
    fn test_input_matching_nothing_warns() -> Result<()> {
        let empty_dir = tempdir()?;

        cargo_bin_cmd!()
            .arg(format!("{}", empty_dir.path().to_string_lossy()))
            .arg(format!("{}/*", empty_dir.path().to_string_lossy()))
            .arg("non-existing-path/*")
            .arg("*.non-existing-extension")
            .arg("non-existing-file-name???")
            .assert()
            .success()
            .stderr(contains("No files found").count(5));

        Ok(())
    }

    // An input which is invalid (no permission directory or invalid glob)
    // should fail as a CLI error, not a link checking error.
    #[test]
    fn test_invalid_user_input_source() -> Result<()> {
        cargo_bin_cmd!()
            .arg("http://website.invalid")
            .assert()
            .failure()
            .code(1);

        // maybe test with a directory with no write permissions? but there
        // doesn't seem to be an equivalent to chmod on the windows API:
        // https://doc.rust-lang.org/std/fs/struct.Permissions.html

        cargo_bin_cmd!()
            .arg("invalid-glob[")
            .assert()
            .failure()
            .code(1);

        Ok(())
    }

    /// Invalid glob patterns should be checked and reported as a CLI parsing
    /// error before link checking.
    #[test]
    fn test_invalid_glob_fails_parse() {
        cargo_bin_cmd!()
            .arg("invalid-unmatched-brackets[")
            .assert()
            .stderr(contains("Cannot parse input"))
            .failure()
            .code(1); // cli parsing error code
    }

    /// Preprocessing with `cat` is like an identity function because it
    /// outputs its input without any changes.
    #[test]
    fn test_pre_cat() {
        let file = fixtures_path!().join("TEST.md");
        let pre_with_cat = cargo_bin_cmd!()
            .arg("--preprocess")
            .arg("cat")
            .arg("--dump")
            .arg(&file)
            .assert()
            .success();

        let no_pre = cargo_bin_cmd!()
            .arg("--dump")
            .arg(&file)
            .assert()
            .success()
            .get_output()
            .stdout
            .lines()
            .map(|line| line.unwrap().to_string())
            .collect();

        assert_lines_eq(pre_with_cat, no_pre);
    }

    #[test]
    fn test_pre_invalid_command() {
        let file = fixtures_path!().join("TEST.md");
        cargo_bin_cmd!()
            .arg("--preprocess")
            .arg("program does not exist")
            .arg(file)
            .assert()
            .failure()
            .code(2)
            .stdout(contains("Preprocessor command 'program does not exist' failed: could not start: No such file or directory"));
    }

    #[test]
    fn test_pre_error() {
        let file = fixtures_path!().join("TEST.md");
        let script = fixtures_path!().join("pre").join("no_error_message.sh");
        cargo_bin_cmd!()
            .arg("--preprocess")
            .arg(&script)
            .arg(&file)
            .assert()
            .failure()
            .code(2)
            .stdout(contains(format!(
                "Preprocessor command '{}' failed: exited with non-zero code: <empty stderr>",
                script.as_os_str().to_str().unwrap()
            )));

        let script = fixtures_path!().join("pre").join("error_message.sh");
        cargo_bin_cmd!()
            .arg("--preprocess")
            .arg(&script)
            .arg(file)
            .assert()
            .failure()
            .code(2)
            .stdout(contains(format!(
                "Preprocessor command '{}' failed: exited with non-zero code: Some error message",
                script.as_os_str().to_str().unwrap()
            )));
    }

    #[test]
    fn test_mdx_file() {
        let file = fixtures_path!().join("mdx").join("test.mdx");
        cargo_bin_cmd!()
            .arg("--dump")
            .arg(&file)
            .assert()
            .success()
            .stdout(contains("https://example.com"));
    }

    #[test]
    fn test_local_base_url_bug_1896() -> Result<()> {
        let dir = tempdir()?;

        cargo_bin_cmd!()
            .arg("-")
            .arg("--dump")
            .arg("--base-url")
            .arg(dir.path())
            .arg("--default-extension")
            .arg("md")
            .write_stdin("[a](b.html#a)")
            .assert()
            .success()
            .stdout(contains("b.html#a"));

        Ok(())
    }
}
