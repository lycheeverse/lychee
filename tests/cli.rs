#[cfg(test)]
mod cli {
    use assert_cmd::Command;
    use predicates::str::contains;
    use std::path::Path;

    #[test]
    fn test_exclude_all_private() {
        let mut cmd =
            Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name");

        let test_all_private_path = Path::new(module_path!())
            .parent()
            .unwrap()
            .join("fixtures")
            .join("TEST_ALL_PRIVATE.md");

        // assert that the command runs OK, and that it excluded all the links
        cmd.env("GITHUB_TOKEN", "invalid-token")
            .arg("--exclude-all-private")
            .arg("--verbose")
            .arg(test_all_private_path)
            .assert()
            .success()
            .stdout(contains("Found: 7"))
            .stdout(contains("Excluded: 7"))
            .stdout(contains("Successful: 0"))
            .stdout(contains("Errors: 0"));
    }
}
