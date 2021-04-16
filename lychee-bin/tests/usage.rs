#[cfg(all(test, unix))]
mod readme {
    use std::{
        fs::File,
        io::{BufReader, Read},
        path::Path,
    };

    use assert_cmd::Command;
    use pretty_assertions::assert_eq;

    fn main_command() -> Command {
        // this gets the "main" binary name (e.g. `lychee`)
        Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name")
    }

    fn load_readme_text() -> String {
        let readme_path = Path::new(env!("CARGO_MANIFEST_DIR"))
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
    #[tokio::test]
    #[cfg_attr(not(feature = "default"), ignore)]
    async fn test_readme_usage_up_to_date() {
        let mut cmd = main_command();

        let result = cmd.env_clear().arg("--help").assert().success();
        let help_output = std::str::from_utf8(&result.get_output().stdout)
            .expect("Invalid utf8 output for `--help`");
        let readme = load_readme_text();

        const BACKTICKS_OFFSET: usize = 9; // marker: ```ignore
        const NEWLINE_OFFSET: usize = 1;

        let usage_start = BACKTICKS_OFFSET
            + NEWLINE_OFFSET
            + readme
                .find("```ignore\nUSAGE:\n")
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
