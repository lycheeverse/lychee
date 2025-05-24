//! The rest of the integration tests make heavy use of example domains, so
//! we use a separate module for testing that the exclusion of these domains
//! works as expected for normal users.
#[cfg(test)]
mod cli {
    use std::{
        error::Error,
        path::{Path, PathBuf},
    };

    use assert_cmd::Command;
    use predicates::str::contains;

    type Result<T> = std::result::Result<T, Box<dyn Error>>;

    fn fixtures_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("fixtures")
    }

    fn main_command() -> Command {
        // this gets the "main" binary name (e.g. `lychee`)
        Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name")
    }

    #[test]
    #[cfg(not(feature = "check_example_domains"))]
    fn test_exclude_example_domains() -> Result<()> {
        let mut cmd = main_command();
        let input = fixtures_path().join("TEST_EXAMPLE_DOMAINS.md");

        let cmd = cmd
            .arg(input)
            .arg("--include-mail")
            .arg("--dump")
            .assert()
            .success()
            .stdout(contains("mail@somedomain.com"))
            .stdout(contains("foo@bar.dev"))
            .stdout(contains("https://github.com/rust-lang/rust"))
            .stdout(contains("https://www.rust-lang.org/"));

        let output = cmd.get_output();
        let output = std::str::from_utf8(&output.stdout).unwrap();
        assert_eq!(output.lines().count(), 4);

        Ok(())
    }

    #[test]
    fn test_do_not_exclude_false_positive_example_domains() -> Result<()> {
        let mut cmd = main_command();
        let input = fixtures_path().join("TEST_EXAMPLE_DOMAINS_FALSE_POSITIVES.md");

        let cmd = cmd
            .arg(input)
            .arg("--include-mail")
            .arg("--dump")
            .assert()
            .success()
            .stdout(contains("https://examples.com"))
            .stdout(contains("https://texample.net"))
            .stdout(contains("http://gobyexample.com"));

        let output = cmd.get_output();
        let output = std::str::from_utf8(&output.stdout).unwrap();
        assert_eq!(output.lines().count(), 8);

        Ok(())
    }
}
