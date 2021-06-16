#[cfg(test)]
mod cli {
    use std::{fs::File, io::Write};

    use assert_cmd::Command;
    use lychee_lib::Result;
    use predicates::str::contains;

    fn main_command() -> Command {
        // this gets the "main" binary name (e.g. `lychee`)
        Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name")
    }

    #[tokio::test]
    async fn test_local_file() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let index_path = dir.path().join("index.html");
        let mut index = File::create(&index_path)?;
        writeln!(index, r#"<a href="./foo.html">Foo</a>"#)?;

        let foo_path = dir.path().join("foo.html");
        let mut foo = File::create(&foo_path)?;
        writeln!(foo, r#"<a href="https://example.org">example</a>"#)?;

        let mut cmd = main_command();
        cmd.arg(index_path)
            .arg("--no-progress")
            .arg("--verbose")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("Total............1"))
            .stdout(contains("example.org"));

        Ok(())
    }
}
