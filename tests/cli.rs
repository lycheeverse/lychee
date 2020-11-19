#[cfg(test)]
mod cli {
    use assert_cmd::Command;
    use predicates::str::contains;
    use std::path::Path;

    #[test]
    fn test_exclude_all_private() {
        // this gets the "main" binary name (e.g. `lychee`)
        let mut cmd =
            Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name");

        let test_all_private_path = Path::new(module_path!())
            .parent()
            .unwrap()
            .join("fixtures")
            .join("TEST_ALL_PRIVATE.md");

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
        let mut cmd =
            Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name");

        let test_github_path = Path::new(module_path!())
            .parent()
            .unwrap()
            .join("fixtures")
            .join("TEST_GITHUB.md");

        cmd.arg("--verbose")
            .arg(test_github_path)
            .assert()
            .success()
            .stdout(contains("Total: 1"))
            .stdout(contains("Excluded: 0"))
            .stdout(contains("Successful: 1"))
            .stdout(contains("Errors: 0"));
    }

    #[test]
    fn test_failure_404_link() {
        let mut cmd =
            Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name");

        let test_404_path = Path::new(module_path!())
            .parent()
            .unwrap()
            .join("fixtures")
            .join("TEST_404.md");

        cmd.arg(test_404_path).assert().failure().code(2);
    }

    #[test]
    fn test_failure_github_404_no_token() {
        let mut cmd =
            Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name");

        let test_github_404_path = Path::new(module_path!())
            .parent()
            .unwrap()
            .join("fixtures")
            .join("TEST_GITHUB_404.md");

        cmd.arg(test_github_404_path)
            .assert()
            .failure()
            .code(2)
            .stdout(contains("https://github.com/mre/idiomatic-rust-doesnt-exist-man \
            (GitHub token not specified. To check GitHub links reliably, use `--github-token` flag / `GITHUB_TOKEN` env var.)"));
    }
}
