//! The integration tests which concern recursion and features leading up
//! to it.
#[cfg(test)]
mod cli {
    use std::error::Error;
    use tempfile::tempdir;

    use assert_cmd::cargo::cargo_bin_cmd;
    use predicates::str::contains;

    type Result<T> = std::result::Result<T, Box<dyn Error>>;

    #[test]
    fn test_recursion_does_not_deadlock_when_exceeding_max_concurrency() -> Result<()> {
        let dir = tempdir()?;

        let mut num_links_total = 0;
        for i in 0..=10 {
            // each test file links to unique recursive files to avoid caching.
            let mut links = (0..15)
                .map(|j| format!("{j}th-link-in-file-{i}.html"))
                .collect::<Vec<_>>();

            for l in &links {
                std::fs::write(dir.path().join(l), "")?;
            }

            if i != 10 {
                links.push(format!("test-file-{}.html", i + 1));
            };

            let html = links
                .iter()
                .map(|p| format!(r#"<a href="{p}"> x </a>"#))
                .collect::<Vec<_>>()
                .join("\n");

            num_links_total += links.len();
            std::fs::write(dir.path().join(format!("test-file-{i}.html")), html)?;
        }

        // should not deadlock, and all links should succeed.
        let _cmd = cargo_bin_cmd!()
            .arg(dir.path())
            .arg("--max-concurrency=1")
            .timeout(std::time::Duration::from_secs(3))
            .assert()
            .success()
            .stdout(contains(format!("{num_links_total} OK")))
            .stdout(contains("0 Errors"));

        Ok(())
    }
}
