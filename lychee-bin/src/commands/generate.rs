//! A module to generate lychee-bin related output for usability purposes.
//! The generated data is not related to the main use-cases of lychee
//! such as link checking but for usability purposes, such as the manual page
//! and shell completions.

use anyhow::Result;
use clap::{CommandFactory, crate_authors};
use clap_mangen::{
    Man,
    roff::{Roff, roman},
};
use serde::Deserialize;
use strum::{Display, EnumIter, EnumString, VariantNames};

use crate::LycheeOptions;

const CONTRIBUTOR_THANK_NOTE: &str = "\n\nA huge thank you to all the wonderful contributors who helped make this project a success.";

const BUG_SECTION: &str =
    "Report any bugs or questions to <https://github.com/lycheeverse/lychee/issues/>

Questions can also be asked on <https://github.com/lycheeverse/lychee/discussions>";

type Description = &'static str;
type Commands = &'static [&'static str];
type Example = (Description, Commands);

/// Used to render the EXAMPLES section in the man page.
const EXAMPLES: &[Example] = &[
    (
        "Check all links in supported files by specifying a directory",
        &["lychee ."],
    ),
    (
        "Specify files explicitly or use glob patterns",
        &[
            "lychee README.md test.html info.txt",
            "lychee 'public/**/*.html' '*.md'",
        ],
    ),
    (
        "Check all links on a website",
        &["lychee https://example.com"],
    ),
    (
        "Check links from stdin",
        &[
            "cat test.md | lychee -",
            "echo 'https://example.com' | lychee -",
        ],
    ),
    (
        "Links can be excluded and included with regular expressions",
        &["lychee --exclude '^https?://blog\\.example\\.com' --exclude '\\.(pdf|zip|png|jpg)$' ."],
    ),
    (
        "Further examples can be found in the online documentation at <https://lychee.cli.rs>",
        &[],
    ),
];

const EXIT_CODE_SECTION: &str = "
0   Success. The operation was completed successfully as instructed.

1   Missing inputs or any unexpected runtime failures or configuration errors

2   Link check failures. At least one non-excluded link failed the check.

3   Encountered errors in the config file.
";

/// What to generate when providing the --generate flag
#[derive(Debug, Deserialize, Clone, Display, EnumIter, EnumString, VariantNames, PartialEq)]
#[non_exhaustive]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub(crate) enum GenerateMode {
    /// Generate roff used for the man page
    Man,
}

/// Generate special output according to the [`GenerateMode`]
pub(crate) fn generate(mode: &GenerateMode) -> Result<String> {
    match mode {
        GenerateMode::Man => man_page(),
    }
}

/// Generate the lychee man page in roff format using [`clap_mangen`]
fn man_page() -> Result<String> {
    let authors = crate_authors!("\n\n").to_owned() + CONTRIBUTOR_THANK_NOTE;

    let man = Man::new(LycheeOptions::command().author(authors)).date(env!("GIT_DATE"));
    let buffer = &mut Vec::default();

    // Manually customise `Man::render` (see https://github.com/clap-rs/clap/issues/3354)
    man.render_title(buffer)?;
    man.render_name_section(buffer)?;
    man.render_synopsis_section(buffer)?;
    man.render_description_section(buffer)?;
    man.render_options_section(buffer)?;
    render_examples(buffer)?;
    render_exit_codes(buffer)?;
    render_bug_reporting(buffer)?;
    man.render_version_section(buffer)?;
    man.render_authors_section(buffer)?;

    Ok(std::str::from_utf8(buffer)?.to_owned())
}

fn render_exit_codes(buffer: &mut Vec<u8>) -> Result<()> {
    render_section("EXIT CODES", EXIT_CODE_SECTION, buffer)
}

fn render_examples(buffer: &mut Vec<u8>) -> Result<()> {
    let section = EXAMPLES
        .iter()
        .map(|(description, examples)| {
            let examples = examples
                .iter()
                .map(|example| format!("    $ {example}"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{description}\n\n{examples}")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    render_section("EXAMPLES", &section, buffer)
}

fn render_bug_reporting(buffer: &mut Vec<u8>) -> Result<()> {
    render_section("REPORTING BUGS", BUG_SECTION, buffer)
}

fn render_section(title: &str, content: &str, buffer: &mut Vec<u8>) -> Result<()> {
    let mut roff = Roff::default();
    roff.control("SH", [title]);
    roff.text([roman(content)]);
    roff.to_writer(buffer)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::man_page;
    use crate::generate::{CONTRIBUTOR_THANK_NOTE, EXIT_CODE_SECTION};
    use anyhow::Result;

    #[test]
    fn test_man_page() -> Result<()> {
        let roff = man_page()?;

        // Must contain description
        assert!(roff.contains("lychee \\- A fast, async link checker"));
        assert!(roff.contains(
            "lychee is a fast, asynchronous link checker which detects broken URLs and mail addresses in local files and websites. It supports Markdown and HTML and works well with many plain text file formats."
        ));
        assert!(
            roff.contains("lychee is powered by lychee\\-lib, the Rust library for link checking.")
        );

        // Must contain authors and thank note
        assert!(roff.contains("Matthias Endler"));
        assert!(roff.contains(CONTRIBUTOR_THANK_NOTE));

        // Flags should normally occur exactly twice.
        // Once in SYNOPSIS and once in OPTIONS.
        assert_eq!(roff.matches("\\-\\-version").count(), 2);
        Ok(())
    }

    /// Test that the Exit Codes section in `README.md` is up to date with
    /// lychee's manual page.
    #[test]
    #[cfg(unix)]
    fn test_readme_exit_codes_up_to_date() -> Result<(), Box<dyn std::error::Error>> {
        use test_utils::load_readme_text;

        const BEGIN: &str = "### Exit codes";
        const END: &str = "# ";

        let readme = load_readme_text!();
        let start = readme.find(BEGIN).ok_or("Beginning not found in README")? + BEGIN.len();
        let end = readme[start..].find(END).ok_or("End not found in README")? - END.len();

        let section = &readme[start..start + end];
        assert_eq!(
            filter_empty_lines(section),
            filter_empty_lines(EXIT_CODE_SECTION)
        );

        Ok(())
    }

    fn filter_empty_lines(s: &str) -> String {
        s.lines()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
