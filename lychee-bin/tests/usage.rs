#[cfg(test)]
mod readme {
    use std::{fs, path::Path};

    use assert_cmd::Command;
    use pretty_assertions::assert_eq;

    const USAGE_STRING: &str = "Usage: lychee [OPTIONS] [inputs]...\n";

    fn main_command() -> Command {
        // this gets the "main" binary name (e.g. `lychee`)
        Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name")
    }

    fn load_readme_text() -> String {
        let readme_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("README.md");
        fs::read_to_string(readme_path).unwrap()
    }

    /// Remove line `[default: lychee/x.y.z]` from the string
    fn remove_lychee_version_line(string: &str) -> String {
        string
            .lines()
            .filter(|line| !line.contains("[default: lychee/"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn trim_empty_lines(str: &str) -> String {
        str.lines()
            .map(|line| if line.trim().is_empty() { "" } else { line })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Test that the USAGE section in `README.md` is up to date with
    /// `lychee --help`.
    /// Only unix: might not work with windows CRLF line-endings returned from
    /// process output (making it fully portable would probably require more
    /// involved parsing).
    #[test]
    #[cfg(unix)]
    fn test_readme_usage_up_to_date() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = main_command();

        let help_cmd = cmd.env_clear().arg("--help").assert().success();
        let help_output = std::str::from_utf8(&help_cmd.get_output().stdout)?;
        let usage_in_help_start = help_output
            .find(USAGE_STRING)
            .ok_or("Usage not found in help")?;
        let usage_in_help = &help_output[usage_in_help_start..];

        let usage_in_help = trim_empty_lines(&remove_lychee_version_line(usage_in_help));
        let readme = load_readme_text();
        let usage_start = readme
            .find(USAGE_STRING)
            .ok_or("Usage not found in README")?;
        let usage_end = readme[usage_start..]
            .find("\n```")
            .ok_or("End of usage not found in README")?;
        let usage_in_readme = &readme[usage_start..usage_start + usage_end];
        let usage_in_readme = remove_lychee_version_line(usage_in_readme);

        assert_eq!(usage_in_readme, usage_in_help);
        Ok(())
    }
}
