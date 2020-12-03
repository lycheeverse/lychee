#[cfg(test)]
mod cli {
    use anyhow::Result;
    use assert_cmd::Command;
    use lychee::test_utils;
    use predicates::str::contains;
    use std::fs::File;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn main_command() -> Command {
        // this gets the "main" binary name (e.g. `lychee`)
        Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name")
    }

    fn fixtures_path() -> PathBuf {
        Path::new(module_path!()).parent().unwrap().join("fixtures")
    }

    #[test]
    fn test_exclude_all_private() {
        let mut cmd = main_command();

        let test_all_private_path = fixtures_path().join("TEST_ALL_PRIVATE.md");

        // assert that the command runs OK, and that it excluded all the links
        cmd.arg("--exclude-all-private")
            .arg("--verbose")
            .arg(test_all_private_path)
            .assert()
            .success()
            .stdout(contains("Total: 7"))
            .stdout(contains("Excluded: 7"))
            .stdout(contains("Successful: 0"))
            .stdout(contains("Errors: 0"));
    }

    /// Test that a GitHub link can be checked without specifying the token.
    #[test]
    fn test_check_github_no_token() {
        let mut cmd = main_command();
        let test_github_path = fixtures_path().join("TEST_GITHUB.md");

        cmd.arg("--verbose")
            .arg(test_github_path)
            .assert()
            .success()
            .stdout(contains("Total: 1"))
            .stdout(contains("Excluded: 0"))
            .stdout(contains("Successful: 1"))
            .stdout(contains("Errors: 0"));
    }

    #[tokio::test]
    async fn test_failure_404_link() {
        let mut cmd = main_command();
        let mock_server = test_utils::get_mock_server(http::StatusCode::NOT_FOUND).await;
        let dir = tempfile::tempdir().expect("Failed to create tempdir");
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path).expect("Failed to create tempfile");

        writeln!(file, "{}", mock_server.uri()).expect("Failed to write to file");

        cmd.arg(file_path)
            .write_stdin(mock_server.uri())
            .assert()
            .failure()
            .code(2);
    }

    #[test]
    fn test_failure_github_404_no_token() {
        let mut cmd = main_command();
        let test_github_404_path = fixtures_path().join("TEST_GITHUB_404.md");

        cmd.arg(test_github_404_path)
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
        let mock_server = test_utils::get_mock_server(http::StatusCode::OK).await;

        cmd.arg("-")
            .write_stdin(mock_server.uri())
            .assert()
            .success();
    }

    #[tokio::test]
    async fn test_stdin_input_failure() {
        let mut cmd = main_command();
        let mock_server =
            test_utils::get_mock_server(http::StatusCode::INTERNAL_SERVER_ERROR).await;

        cmd.arg("-")
            .write_stdin(mock_server.uri())
            .assert()
            .failure()
            .code(2);
    }

    #[tokio::test]
    async fn test_stdin_input_multiple() {
        let mut cmd = main_command();
        let mock_server_a = test_utils::get_mock_server(http::StatusCode::OK).await;
        let mock_server_b = test_utils::get_mock_server(http::StatusCode::OK).await;

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
                "Error: Failed to read file: `{}`",
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
        let mock_server_a = test_utils::get_mock_server(http::StatusCode::OK).await;
        let mock_server_b = test_utils::get_mock_server(http::StatusCode::OK).await;
        let mut file_a = File::create(dir.path().join("a.md"))?;
        let mut file_b = File::create(dir.path().join("b.md"))?;

        writeln!(file_a, "{}", mock_server_a.uri().as_str())?;
        writeln!(file_b, "{}", mock_server_b.uri().as_str())?;

        cmd.arg(dir.path().join("*.md"))
            .arg("--verbose")
            .assert()
            .success()
            .stdout(contains("Total: 2"));

        Ok(())
    }

    #[cfg(target_os = "linux")] // MacOS and Windows have case-insensitive filesystems
    #[tokio::test]
    async fn test_glob_ignore_case() -> Result<()> {
        let mut cmd = main_command();

        let dir = tempfile::tempdir()?;
        let mock_server_a = test_utils::get_mock_server(http::StatusCode::OK).await;
        let mock_server_b = test_utils::get_mock_server(http::StatusCode::OK).await;
        let mut file_a = File::create(dir.path().join("README.md"))?;
        let mut file_b = File::create(dir.path().join("readme.md"))?;

        writeln!(file_a, "{}", mock_server_a.uri().as_str())?;
        writeln!(file_b, "{}", mock_server_b.uri().as_str())?;

        cmd.arg(dir.path().join("[r]eadme.md"))
            .arg("--verbose")
            .arg("--glob-ignore-case")
            .assert()
            .success()
            .stdout(contains("Total: 2"));

        Ok(())
    }

    #[tokio::test]
    async fn test_glob_recursive() -> Result<()> {
        let mut cmd = main_command();

        let dir = tempfile::tempdir()?;
        let subdir_level_1 = tempfile::tempdir_in(&dir)?;
        let subdir_level_2 = tempfile::tempdir_in(&subdir_level_1)?;

        let mock_server = test_utils::get_mock_server(http::StatusCode::OK).await;
        let mut file = File::create(subdir_level_2.path().join("test.md"))?;

        writeln!(file, "{}", mock_server.uri().as_str())?;

        // ** should be a recursive glob
        cmd.arg(dir.path().join("**/*.md"))
            .arg("--verbose")
            .assert()
            .success()
            .stdout(contains("Total: 1"));

        Ok(())
    }
}
