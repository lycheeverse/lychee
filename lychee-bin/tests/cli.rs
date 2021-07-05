#[cfg(test)]
mod cli {
    use std::{
        fs::{self, File},
        io::Write,
        path::{Path, PathBuf},
    };

    use assert_cmd::Command;
    use http::StatusCode;
    use lychee_lib::Result;
    use predicates::str::contains;
    use pretty_assertions::assert_eq;
    use uuid::Uuid;

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
        timeouts: usize,
        redirects: usize,
        excludes: usize,
        errors: usize,
    }

    impl MockResponseStats {
        fn to_json_str(&self) -> String {
            format!(
                r#"{{
  "total": {},
  "successful": {},
  "failures": {},
  "timeouts": {},
  "redirects": {},
  "excludes": {},
  "errors": {},
  "fail_map": {{}}
}}"#,
                self.total,
                self.successful,
                self.failures,
                self.timeouts,
                self.redirects,
                self.excludes,
                self.errors
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
            .stdout(contains("Total............2"))
            .stdout(contains("Successful.......1"))
            .stdout(contains("Excluded.........1"));
    }

    #[test]
    fn test_quirks() -> Result<()> {
        test_json_output!(
            "TEST_QUIRKS.txt",
            MockResponseStats {
                total: 3,
                successful: 3,
                ..MockResponseStats::default()
            }
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
            .stdout(contains("Total............3"))
            .stdout(contains("Successful.......2"))
            .stdout(contains("Excluded.........1"));
    }

    #[test]
    fn test_repetition() {
        let mut cmd = main_command();
        let test_schemes_path = fixtures_path().join("TEST_REPETITION.txt");

        cmd.arg(&test_schemes_path)
            .env_clear()
            .assert()
            .success()
            .stdout(contains("Total............1"))
            .stdout(contains("Successful.......1"));
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
            .stdout(contains("https://github.com/mre/idiomatic-rust-doesnt-exist-man \
            (GitHub token not specified. To check GitHub links reliably, use `--github-token` flag / `GITHUB_TOKEN` env var.)"));
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
    fn test_missing_file_error() {
        let mut cmd = main_command();
        let filename = format!("non-existing-file-{}", uuid::Uuid::new_v4().to_string());

        cmd.arg(&filename)
            .assert()
            .failure()
            .code(1)
            .stderr(contains(format!(
                "Error: Failed to read file: `{}`, reason: No such file or directory (os error 2)",
                filename
            )));
    }

    #[test]
    fn test_missing_file_ok_if_skip_missing() {
        let mut cmd = main_command();
        let filename = format!("non-existing-file-{}", uuid::Uuid::new_v4().to_string());

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
            .stdout(contains("Total............2"));

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
            .stdout(contains("Total............2"));

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
            .stdout(contains("Total............1"));

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

        let expected = r#"{"total":11,"successful":11,"failures":0,"timeouts":0,"redirects":0,"excludes":0,"errors":0,"fail_map":{}}"#;
        let output = fs::read_to_string(&outfile)?;
        assert_eq!(output.split_whitespace().collect::<String>(), expected);
        fs::remove_file(outfile)?;
        Ok(())
    }
}
