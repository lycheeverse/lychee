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
        File::create(&foo_path)?;

        let mut cmd = main_command();
        cmd.arg(index_path)
            .arg("--no-progress")
            .arg("--verbose")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("foo.html"));

        Ok(())
    }

    #[tokio::test]
    async fn test_local_dir() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let index_path = dir.path().join("index.html");
        let mut index = File::create(&index_path)?;
        writeln!(index, r#"<a href="./foo.html">Foo</a>"#)?;
        writeln!(index, r#"<a href="./bar.md">Bar</a>"#)?;

        let foo_path = dir.path().join("foo.html");
        File::create(&foo_path)?;
        let bar_path = dir.path().join("bar.md");
        File::create(&bar_path)?;

        let mut cmd = main_command();
        cmd.arg(dir.path())
            .arg("--no-progress")
            .arg("--verbose")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("Total............2"))
            .stdout(contains("foo.html"))
            .stdout(contains("bar.md"));

        Ok(())
    }
}
