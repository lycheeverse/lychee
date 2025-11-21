#[cfg(test)]
mod cli {
    use std::{error::Error, fs::File, io::Write};

    use assert_cmd::cargo::cargo_bin_cmd;
    use predicates::str::contains;

    type Result<T> = std::result::Result<T, Box<dyn Error>>;

    #[tokio::test]
    async fn test_local_file() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let index_path = dir.path().join("index.html");
        let mut index = File::create(&index_path)?;
        writeln!(index, r#"<a href="./foo.html">Foo</a>"#)?;

        let foo_path = dir.path().join("foo.html");
        File::create(foo_path)?;

        cargo_bin_cmd!()
            .arg(index_path)
            .arg("--no-progress")
            .arg("--verbose")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stderr(contains("foo.html"));

        Ok(())
    }

    #[tokio::test]
    async fn test_local_dir() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let index_path = dir.path().join("index.html");
        let mut index = File::create(index_path)?;
        writeln!(index, r#"<a href="./foo.html">Foo</a>"#)?;
        writeln!(index, r#"<a href="./bar.md">Bar</a>"#)?;

        let foo_path = dir.path().join("foo.html");
        File::create(foo_path)?;
        let bar_path = dir.path().join("bar.md");
        File::create(bar_path)?;

        cargo_bin_cmd!()
            .arg(dir.path())
            .arg("--no-progress")
            .arg("--verbose")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("2 Total"))
            .stdout(contains("2 OK"))
            .stderr(contains("foo.html"))
            .stderr(contains("bar.md"));

        Ok(())
    }
}
