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
            .stdout(contains("Found: 7"))
            .stdout(contains("Excluded: 7"))
            .stdout(contains("Successful: 0"))
            .stdout(contains("Errors: 0"));
    }

    #[test]
    fn test_warn_github() {
        let mut cmd =
            Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name");

        let test_github_token_path = Path::new(module_path!())
            .parent()
            .unwrap()
            .join("fixtures")
            .join("TEST_GITHUB_TOKEN.md");

        // assert that the command runs OK, and that it excluded all the links
        cmd.arg("--verbose")
            .arg(test_github_token_path)
            .assert()
            .success()
            .stdout(contains("[WARN] GitHub API token (`--github-token` / `GITHUB_TOKEN`) not specified. \
                              This can lead to errors with GitHub links due to rate-limiting on github.com"))
            .stdout(contains("Found: 1"))
            .stdout(contains("Excluded: 0"))
            .stdout(contains("Successful: 1"))
            .stdout(contains("Errors: 0"));
    }
}
