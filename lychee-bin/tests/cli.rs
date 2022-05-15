#[cfg(test)]
mod cli {
    use std::{
        error::Error,
        fs::{self, File},
        io::Write,
        path::{Path, PathBuf},
    };

    use assert_cmd::Command;
    use http::StatusCode;
    use predicates::str::{contains, is_empty};
    use uuid::Uuid;

    type Result<T> = std::result::Result<T, Box<dyn Error>>;

    macro_rules! mock_server {
        ($status:expr $(, $func:tt ($($arg:expr),*))*) => {{
            let mock_server = wiremock::MockServer::start().await;
            let template = wiremock::ResponseTemplate::new(http::StatusCode::from($status));
            let template = template$(.$func($($arg),*))*;
            wiremock::Mock::given(wiremock::matchers::method("GET")).respond_with(template).mount(&mock_server).await;
            mock_server
        }};
    }

    fn main_command() -> Command {
        // this gets the "main" binary name (e.g. `lychee`)
        Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name")
    }

    fn fixtures_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("fixtures")
    }

    #[derive(Default)]
    struct MockResponseStats {
        total: usize,
        successful: usize,
        failures: usize,
        unknown: usize,
        timeouts: usize,
        redirects: usize,
        excludes: usize,
        errors: usize,
        cached: usize,
    }

    impl MockResponseStats {
        fn to_json_str(&self) -> String {
            format!(
                r#"{{
  "total": {},
  "successful": {},
  "failures": {},
  "unknown": {},
  "timeouts": {},
  "redirects": {},
  "excludes": {},
  "errors": {},
  "cached": {},
  "fail_map": {{}}
}}"#,
                self.total,
                self.successful,
                self.failures,
                self.unknown,
                self.timeouts,
                self.redirects,
                self.excludes,
                self.errors,
                self.cached
            )
        }
    }

    macro_rules! test_json_output {
        ($test_file:expr, $expected:expr $(, $arg:expr)*) => {{
            let mut cmd = main_command();
            let test_path = fixtures_path().join($test_file);
            let outfile = format!("{}.json", uuid::Uuid::new_v4());

            let expected = $expected.to_json_str();

            cmd$(.arg($arg))*.arg("--output").arg(&outfile).arg("--format").arg("json").arg(test_path).assert().success();

            let output = std::fs::read_to_string(&outfile)?;
            assert_eq!(output, expected);
            std::fs::remove_file(outfile)?;
            Ok(())
        }};
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
            "--exclude-all-private",
            "--verbose"
        )
    }

    #[test]
    fn test_exclude_email() -> Result<()> {
        test_json_output!(
            "TEST_EMAIL.md",
            MockResponseStats {
                total: 6,
                excludes: 4,
                successful: 2,
                ..MockResponseStats::default()
            },
            "--exclude-mail"
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
    fn test_unsupported_uri_schemes() {
        let mut cmd = main_command();
        let test_schemes_path = fixtures_path().join("TEST_SCHEMES.txt");

        // Exclude file link because it doesn't exist on the filesystem.
        // (File URIs are absolute paths, which we don't have.)
        // Nevertheless, the `file` scheme should be recognized.
        cmd.arg(test_schemes_path)
            .arg("--exclude")
            .arg("file://")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("2 Total"))
            .stdout(contains("1 OK"))
            .stdout(contains("1 Excluded"));
    }

    #[test]
    fn test_resolve_paths() {
        let mut cmd = main_command();
        let offline_dir = fixtures_path().join("offline");

        cmd.arg("--offline")
            .arg("--base")
            .arg(&offline_dir)
            .arg(&offline_dir.join("index.html"))
            .env_clear()
            .assert()
            .success()
            .stdout(contains("3 Total"))
            .stdout(contains("3 OK"));
    }

    #[test]
    fn test_quirks() -> Result<()> {
        test_json_output!(
            "TEST_QUIRKS.txt",
            MockResponseStats {
                total: 3,
                successful: 2,
                excludes: 1,
                ..MockResponseStats::default()
            },
            // Currently getting a 429 with Googlebot.
            // See https://github.com/lycheeverse/lychee/issues/448
            // See https://twitter.com/matthiasendler/status/1479224185125748737
            // TODO: Remove this exclusion in the future
            "--exclude",
            "twitter"
        )
    }

    #[tokio::test]
    async fn test_failure_404_link() -> Result<()> {
        let mock_server = mock_server!(StatusCode::NOT_FOUND);
        let dir = tempfile::tempdir()?;
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path)?;
        writeln!(file, "{}", mock_server.uri())?;

        let mut cmd = main_command();
        cmd.arg(file_path)
            .write_stdin(mock_server.uri())
            .assert()
            .failure()
            .code(2);

        Ok(())
    }

    #[test]
    fn test_schemes() {
        let mut cmd = main_command();
        let test_schemes_path = fixtures_path().join("TEST_SCHEMES.md");

        cmd.arg(test_schemes_path)
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
        let mut cmd = main_command();
        // Repetitions in one file shall all be checked and counted only once.
        let test_schemes_path_1 = fixtures_path().join("TEST_REPETITION_1.txt");

        cmd.arg(&test_schemes_path_1)
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
        let repeated_uris = fixtures_path().join("TEST_REPETITION_*.txt");

        test_json_output!(
            repeated_uris,
            MockResponseStats {
                total: 2,
                cached: 1,
                successful: 2,
                excludes: 0,
                ..MockResponseStats::default()
            },
            "--verbose",
            // Two requests to the same URI may be executed in parallel. As a
            // result, the response might not be cached and the test would be
            // flaky. Therefore limit the concurrency to one request at a time.
            "--max-concurrency",
            "1"
        )
    }

    #[test]
    fn test_failure_github_404_no_token() {
        let mut cmd = main_command();
        let test_github_404_path = fixtures_path().join("TEST_GITHUB_404.md");

        cmd.arg(test_github_404_path)
            .arg("--no-progress")
            .env_clear()
            .assert()
            .failure()
            .code(2)
            .stdout(contains(
                "https://github.com/mre/idiomatic-rust-doesnt-exist-man | Network error: Not Found",
            ))
            .stdout(contains(
                "There were issues with Github URLs. You could try setting a Github token and running lychee again.",
            ));
    }

    #[tokio::test]
    async fn test_stdin_input() {
        let mut cmd = main_command();
        let mock_server = mock_server!(StatusCode::OK);

        cmd.arg("-")
            .write_stdin(mock_server.uri())
            .assert()
            .success();
    }

    #[tokio::test]
    async fn test_stdin_input_failure() {
        let mut cmd = main_command();
        let mock_server = mock_server!(StatusCode::INTERNAL_SERVER_ERROR);

        cmd.arg("-")
            .write_stdin(mock_server.uri())
            .assert()
            .failure()
            .code(2);
    }

    #[tokio::test]
    async fn test_stdin_input_multiple() {
        let mut cmd = main_command();
        let mock_server_a = mock_server!(StatusCode::OK);
        let mock_server_b = mock_server!(StatusCode::OK);

        // this behavior (treating multiple `-` as separate inputs) is the same as most CLI tools
        // that accept `-` as stdin, e.g. `cat`, `bat`, `grep` etc.
        cmd.arg("-")
            .arg("-")
            .write_stdin(mock_server_a.uri())
            .write_stdin(mock_server_b.uri())
            .assert()
            .success();
    }

    #[test]
    fn test_missing_file_ok_if_skip_missing() {
        let mut cmd = main_command();
        let filename = format!("non-existing-file-{}", uuid::Uuid::new_v4());

        cmd.arg(&filename).arg("--skip-missing").assert().success();
    }

    #[tokio::test]
    async fn test_glob() -> Result<()> {
        // using Result to be able to use `?`
        let mut cmd = main_command();

        let dir = tempfile::tempdir()?;
        let mock_server_a = mock_server!(StatusCode::OK);
        let mock_server_b = mock_server!(StatusCode::OK);
        let mut file_a = File::create(dir.path().join("a.md"))?;
        let mut file_b = File::create(dir.path().join("b.md"))?;

        writeln!(file_a, "{}", mock_server_a.uri().as_str())?;
        writeln!(file_b, "{}", mock_server_b.uri().as_str())?;

        cmd.arg(dir.path().join("*.md"))
            .arg("--verbose")
            .assert()
            .success()
            .stdout(contains("2 Total"));

        Ok(())
    }

    #[cfg(target_os = "linux")] // MacOS and Windows have case-insensitive filesystems
    #[tokio::test]
    async fn test_glob_ignore_case() -> Result<()> {
        let mut cmd = main_command();

        let dir = tempfile::tempdir()?;
        let mock_server_a = mock_server!(StatusCode::OK);
        let mock_server_b = mock_server!(StatusCode::OK);
        let mut file_a = File::create(dir.path().join("README.md"))?;
        let mut file_b = File::create(dir.path().join("readme.md"))?;

        writeln!(file_a, "{}", mock_server_a.uri().as_str())?;
        writeln!(file_b, "{}", mock_server_b.uri().as_str())?;

        cmd.arg(dir.path().join("[r]eadme.md"))
            .arg("--verbose")
            .arg("--glob-ignore-case")
            .assert()
            .success()
            .stdout(contains("2 Total"));

        Ok(())
    }

    #[tokio::test]
    async fn test_glob_recursive() -> Result<()> {
        let mut cmd = main_command();

        let dir = tempfile::tempdir()?;
        let subdir_level_1 = tempfile::tempdir_in(&dir)?;
        let subdir_level_2 = tempfile::tempdir_in(&subdir_level_1)?;

        let mock_server = mock_server!(StatusCode::OK);
        let mut file = File::create(subdir_level_2.path().join("test.md"))?;

        writeln!(file, "{}", mock_server.uri().as_str())?;

        // ** should be a recursive glob
        cmd.arg(dir.path().join("**/*.md"))
            .arg("--verbose")
            .assert()
            .success()
            .stdout(contains("1 Total"));

        Ok(())
    }

    /// Test formatted file output
    #[test]
    fn test_formatted_file_output() -> Result<()> {
        let mut cmd = main_command();
        let test_path = fixtures_path().join("TEST.md");
        let outfile = format!("{}.json", Uuid::new_v4());

        cmd.arg("--output")
            .arg(&outfile)
            .arg("--format")
            .arg("json")
            .arg(test_path)
            .assert()
            .success();

        let expected = r#"{"total":11,"successful":11,"failures":0,"unknown":0,"timeouts":0,"redirects":0,"excludes":0,"errors":0,"cached":0,"fail_map":{}}"#;
        let output = fs::read_to_string(&outfile)?;
        assert_eq!(output.split_whitespace().collect::<String>(), expected);
        fs::remove_file(outfile)?;
        Ok(())
    }

    /// Test excludes
    #[test]
    fn test_exclude_wildcard() -> Result<()> {
        let mut cmd = main_command();
        let test_path = fixtures_path().join("TEST.md");

        cmd.arg(test_path)
            .arg("--exclude")
            .arg(".*")
            .assert()
            .success()
            .stdout(contains("11 Excluded"));

        Ok(())
    }

    #[test]
    fn test_exclude_multiple_urls() -> Result<()> {
        let mut cmd = main_command();
        let test_path = fixtures_path().join("TEST.md");

        cmd.arg(test_path)
            .arg("--exclude")
            .arg("https://en.wikipedia.org/*")
            .arg("https://ldra.com/")
            .assert()
            .success()
            .stdout(contains("2 Excluded"));

        Ok(())
    }

    #[test]
    fn test_exclude_file() -> Result<()> {
        let mut cmd = main_command();
        let test_path = fixtures_path().join("TEST.md");
        let excludes_path = fixtures_path().join("TEST_EXCLUDE_1.txt");

        cmd.arg(test_path)
            .arg("--exclude-file")
            .arg(excludes_path)
            .assert()
            .success()
            .stdout(contains("2 Excluded"));

        Ok(())
    }

    #[test]
    fn test_multiple_exclude_files() -> Result<()> {
        let mut cmd = main_command();
        let test_path = fixtures_path().join("TEST.md");
        let excludes_path1 = fixtures_path().join("TEST_EXCLUDE_1.txt");
        let excludes_path2 = fixtures_path().join("TEST_EXCLUDE_2.txt");

        cmd.arg(test_path)
            .arg("--exclude-file")
            .arg(excludes_path1)
            .arg(excludes_path2)
            .assert()
            .success()
            .stdout(contains("3 Excluded"));

        Ok(())
    }

    #[tokio::test]
    async fn test_example_config() -> Result<()> {
        let mock_server = mock_server!(StatusCode::OK);
        let mut cmd = main_command();
        cmd.arg("--config")
            .arg("lychee.example.toml")
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
    async fn test_empty_config() -> Result<()> {
        let mock_server = mock_server!(StatusCode::OK);
        let config = fixtures_path().join("configs").join("empty.toml");
        let mut cmd = main_command();
        cmd.arg("--config")
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
    async fn test_cache_config() -> Result<()> {
        let mock_server = mock_server!(StatusCode::OK);
        let config = fixtures_path().join("configs").join("cache.toml");
        let mut cmd = main_command();
        cmd.arg("--config")
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
        let config = fixtures_path().join("configs").join("invalid.toml");
        let mut cmd = main_command();
        cmd.arg("--config")
            .arg(config)
            .arg("-")
            .env_clear()
            .assert()
            .failure();
    }

    #[test]
    fn test_lycheeignore_file() -> Result<()> {
        let mut cmd = main_command();
        let test_path = fixtures_path().join("ignore");

        let cmd = cmd
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
        let mut cmd = main_command();
        let test_path = fixtures_path().join("ignore");
        let excludes_path = test_path.join("normal-exclude-file");

        cmd.current_dir(test_path)
            .arg("TEST.md")
            .arg("--exclude-file")
            .arg(excludes_path)
            .assert()
            .success()
            .stdout(contains("8 Total"))
            .stdout(contains("6 Excluded"));

        Ok(())
    }

    #[test]
    fn test_include_verbatim() -> Result<()> {
        let mut cmd = main_command();
        let input = fixtures_path().join("TEST_CODE_BLOCKS.md");

        cmd.arg("--include-verbatim")
            .arg(input)
            .arg("--dump")
            .assert()
            .success()
            .stdout(contains("http://127.0.0.1/block"))
            .stdout(contains("http://127.0.0.1/inline"))
            .stdout(contains("http://127.0.0.1/bash"));

        Ok(())
    }

    #[test]
    fn test_exclude_verbatim() -> Result<()> {
        let mut cmd = main_command();
        let input = fixtures_path().join("TEST_CODE_BLOCKS.md");

        cmd.arg(input)
            .arg("--dump")
            .assert()
            .success()
            .stdout(is_empty());

        Ok(())
    }

    #[test]
    fn test_require_https() -> Result<()> {
        let mut cmd = main_command();
        let test_path = fixtures_path().join("TEST_HTTP.html");
        cmd.arg(&test_path).assert().success();

        let mut cmd = main_command();
        cmd.arg("--require-https").arg(test_path).assert().failure();

        Ok(())
    }

    /// If base-dir is not set, don't throw an error in case we encounter
    /// an absolute local link within a file (e.g. `/about`).
    #[test]
    fn test_ignore_absolute_local_links_without_base() -> Result<()> {
        let mut cmd = main_command();

        let offline_dir = fixtures_path().join("offline");

        cmd.arg("--offline")
            .arg(&offline_dir.join("index.html"))
            .env_clear()
            .assert()
            .success()
            .stdout(contains("0 Total"));

        Ok(())
    }

    #[test]
    fn test_inputs_without_scheme() -> Result<()> {
        let test_path = fixtures_path().join("TEST_HTTP.html");
        let mut cmd = main_command();

        cmd.arg("--dump")
            .arg("example.com")
            .arg(&test_path)
            .arg("https://example.org")
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn test_print_excluded_links_in_verbose_mode() -> Result<()> {
        let test_path = fixtures_path().join("TEST_DUMP_EXCLUDE.txt");
        let mut cmd = main_command();

        cmd.arg("--dump")
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
        Ok(())
    }

    #[test]
    fn test_remap_uri() -> Result<()> {
        let mut cmd = main_command();

        cmd.arg("--dump")
            .arg("--remap")
            .arg("https://example.com http://127.0.0.1:8080")
            .arg("--remap")
            .arg("https://example.org https://staging.example.com")
            .arg("--remap")
            .arg("../../issues https://github.com/usnistgov/OSCAL/issues")
            .arg("--")
            .arg("-")
            .write_stdin("file://../../issues\nhttps://example.com\nhttps://example.org\nhttps://example.net\n")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("https://github.com/usnistgov/OSCAL/issues"))
            .stdout(contains("http://127.0.0.1:8080/"))
            .stdout(contains("https://staging.example.com/"))
            .stdout(contains("https://example.net/"));

        Ok(())
    }
}
