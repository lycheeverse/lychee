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
    fn test_dump_inputs() -> Result<()> {
        let mut cmd = main_command();
        let input = fixtures_path().join("dump-inputs");

        let cmd = cmd
            .arg(input)
            .arg("--dump-inputs")
            .assert()
            .success()
            .stdout(contains("fixtures/dump-inputs/folder/file.html"))
            .stdout(contains("fixtures/dump-inputs/folder/file.md"))
            .stdout(contains("fixtures/dump-inputs/file.html"))
            .stdout(contains("fixtures/dump-inputs/file.md"));

        let output = cmd.get_output();
        let output = std::str::from_utf8(&output.stdout).unwrap();
        assert_eq!(output.lines().count(), 4);

        Ok(())
    }
}
