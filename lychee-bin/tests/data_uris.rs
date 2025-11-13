//! The rest of the integration tests make heavy use of example domains, so
//! we use a separate module for testing that the exclusion of these domains
//! works as expected for normal users.
#[cfg(test)]
mod cli {
    use std::path::{Path, PathBuf};

    use assert_cmd::cargo::cargo_bin_cmd;
    use predicates::str::contains;

    fn fixtures_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("fixtures")
    }

    #[test]
    fn test_dont_dump_data_uris_by_default() {
        let input = fixtures_path().join("TEST_DATA_URIS.html");

        let cmd = cargo_bin_cmd!()
            .arg(input)
            .arg("--dump")
            .assert()
            .success()
            .stdout(contains("http://localhost/assets/img/bg-water.webp"));

        let output = cmd.get_output();
        let output = std::str::from_utf8(&output.stdout).unwrap();
        assert_eq!(output.lines().count(), 1);
    }

    #[test]
    fn test_dump_data_uris_in_verbose_mode() {
        let input = fixtures_path().join("TEST_DATA_URIS.html");

        let cmd = cargo_bin_cmd!()
            .arg(input)
            .arg("--dump")
            .arg("--verbose")
            .assert()
            .success()
            .stdout(contains("http://www.w3.org/2000/svg"))
            .stdout(contains(
                "data:image/svg+xml,%3csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 147 40'",
            ))
            .stdout(contains("data:text/plain;base64,SGVsbG8sIFdvcmxkIQ=="))
            .stdout(contains("http://localhost/assets/img/bg-water.webp"))
            .stdout(contains("data:,Hello%2C%20World%21"));

        let output = cmd.get_output();
        let output = std::str::from_utf8(&output.stdout).unwrap();
        assert_eq!(output.lines().count(), 5);
    }
}
