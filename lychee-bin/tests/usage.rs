#[cfg(test)]
mod readme {
    use assert_cmd::Command;
    use pretty_assertions::assert_eq;
    use regex::Regex;
    use test_utils::load_readme_text;
    use test_utils::main_command;

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
        const BEGIN: &str = "```help-message\n";
        let mut cmd = main_command!();

        let help_cmd = cmd.env_clear().arg("--help").assert().success();
        let usage_in_help = std::str::from_utf8(&help_cmd.get_output().stdout)?;

        let usage_in_help = trim_empty_lines(&remove_lychee_version_line(usage_in_help));
        let readme = load_readme_text!();
        let usage_start = readme.find(BEGIN).ok_or("Usage not found in README")? + BEGIN.len();
        let usage_end = readme[usage_start..]
            .find("\n```")
            .ok_or("End of usage not found in README")?;
        let usage_in_readme = &readme[usage_start..usage_start + usage_end];
        let usage_in_readme = remove_lychee_version_line(usage_in_readme);

        assert_eq!(usage_in_readme, usage_in_help);
        Ok(())
    }

    /// Test that all the arguments yielded by `lychee --help`
    /// are ordered alphabetically for better usability.
    /// This behaviour aligns with cURL. (see `man curl`)
    #[test]
    #[cfg(unix)]
    fn test_arguments_ordered_alphabetically() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = main_command!();
        let help_cmd = cmd.env_clear().arg("--help").assert().success();
        let help_text = std::str::from_utf8(&help_cmd.get_output().stdout)?;

        let regex = test_utils::arg_regex_help!()?;
        let arguments: Vec<&str> = help_text
            .lines()
            .filter_map(|line| {
                let captures = regex.captures(line)?;
                captures
                    .name("short")
                    .or_else(|| captures.name("long"))
                    .map(|m| m.as_str())
            })
            .collect();

        let mut sorted = arguments.clone();
        sorted.sort_by_key(|arg| arg.to_lowercase());

        if arguments != sorted {
            // Find all positions where order differs
            let mismatches: Vec<_> = arguments
                .iter()
                .zip(&sorted)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, (actual, expected))| format!("  [{i}] '{actual}' should be '{expected}'"))
                .collect();

            panic!(
                "\nArguments are not sorted alphabetically!\n\nMismatches:\n{}\n\nFull actual order:\n{:?}\n\nFull expected order:\n{:?}",
                mismatches.join("\n"),
                arguments,
                sorted
            );
        }

        Ok(())
    }
}
