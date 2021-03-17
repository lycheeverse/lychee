#[cfg(test)]
mod readme {
    use assert_cmd::Command;
    use std::fs::File;
    use std::io::{BufReader, Read};
    use std::path::Path;

    fn main_command() -> Command {
        // this gets the "main" binary name (e.g. `lychee`)
        Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name")
    }

    fn load_readme_text() -> String {
        let readme_path = Path::new(module_path!())
            .parent()
            .unwrap()
            .join("README.md");
        let file = File::open(readme_path).expect("Couldn't open README.md");
        let mut buf_reader = BufReader::new(file);
        let mut text = String::new();

        buf_reader
            .read_to_string(&mut text)
            .expect("Unable to read README.md file contents");

        text
    }

    /// Test that the USAGE section in `README.md` is up to date with
    /// `lychee --help`.
    /// Only unix: might not work with windows CRLF line-endings returned from
    /// process output (making it fully portable would probably require more
    /// involved parsing).
    #[test]
    #[cfg(unix)]
    fn test_readme_usage_up_to_date() {
        let mut cmd = main_command();

        let result = cmd.arg("--help").assert().success();
        let help_output = std::str::from_utf8(&result.get_output().stdout)
            .expect("Invalid utf8 output for `--help`");
        let readme = load_readme_text();

        const BACKTICKS_OFFSET: usize = 3; // marker: ```
        const NEWLINE_OFFSET: usize = 1;

        let usage_start = BACKTICKS_OFFSET
            + NEWLINE_OFFSET
            + readme
                .find("```\nUSAGE:\n")
                .expect("Couldn't find USAGE section in README.md");

        let usage_end = readme[usage_start..]
            .find("\n```")
            .expect("Couldn't find USAGE section end in README.md");

        // include final newline in usage text
        let usage_in_readme = &readme[usage_start..usage_start + usage_end + NEWLINE_OFFSET];

        let usage_in_help_start = help_output
            .find("USAGE:\n")
            .expect("Couldn't find USAGE section in `--help` output");
        let usage_in_help = &help_output[usage_in_help_start..];

        assert_eq!(usage_in_readme, usage_in_help);
    }
}
