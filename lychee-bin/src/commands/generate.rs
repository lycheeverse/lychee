//! A module to generate lychee-bin related output for usability purposes.
//! The generated data is not related to the main use-cases of lychee
//! such as link checking but for usability purposes, such as the manual page
//! and shell completions.

use anyhow::Result;
use clap::{CommandFactory, crate_authors};
use clap_complete::{Shell, generate as generate_completion};
use clap_mangen::{
    Man,
    roff::{Roff, roman},
};
use schemars::JsonSchema;
use serde::Deserialize;
use strum::{Display, EnumIter, EnumString, VariantNames};

use crate::LycheeOptions;
use crate::config::Config;

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
#[derive(
    Debug, Deserialize, Clone, Display, EnumIter, EnumString, VariantNames, PartialEq, JsonSchema,
)]
#[non_exhaustive]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub(crate) enum GenerateMode {
    /// Generate roff used for the man page
    Man,
    /// Generate a JSON schema for the configuration file
    ConfigSchema,
    /// Generate shell completion for Bash
    CompleteBash,
    /// Generate shell completion for Elvish
    CompleteElvish,
    /// Generate shell completion for Fish
    CompleteFish,
    /// Generate shell completion for PowerShell
    CompletePowershell,
    /// Generate shell completion for Zsh
    CompleteZsh,
}

/// Generate special output according to the [`GenerateMode`]
pub(crate) fn generate(mode: &GenerateMode) -> Result<String> {
    match mode {
        GenerateMode::Man => man_page(),
        GenerateMode::ConfigSchema => config_schema(),
        GenerateMode::CompleteBash => shell_completion(Shell::Bash),
        GenerateMode::CompleteElvish => shell_completion(Shell::Elvish),
        GenerateMode::CompleteFish => shell_completion(Shell::Fish),
        GenerateMode::CompletePowershell => shell_completion(Shell::PowerShell),
        GenerateMode::CompleteZsh => shell_completion(Shell::Zsh),
    }
}

/// Generate a JSON schema for the `lychee.toml` configuration file.
///
/// The schema is derived from the [`Config`] type. It can be referenced from a
/// config file via the `$schema` key (or a `taplo`/editor setting) to get
/// autocompletion and validation while editing.
fn config_schema() -> Result<String> {
    let schema = schemars::schema_for!(Config);
    Ok(serde_json::to_string_pretty(&schema)?)
}

/// Generate shell completion for the given shell
fn shell_completion(shell: Shell) -> Result<String> {
    let mut cmd = LycheeOptions::command();
    let mut buffer = Vec::new();
    generate_completion(shell, &mut cmd, "lychee", &mut buffer);
    Ok(String::from_utf8(buffer)?)
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

    Ok(String::from_utf8(buffer.clone())?)
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
    use super::{config_schema, man_page};
    use crate::config::Config;
    use crate::generate::{CONTRIBUTOR_THANK_NOTE, EXIT_CODE_SECTION};
    use anyhow::{Result, bail};
    use serde_json::{Value, json};

    fn config_schema_value() -> Result<Value> {
        Ok(serde_json::from_str(&config_schema()?)?)
    }

    fn toml_to_json(config: &str) -> Result<Value> {
        let config: toml::Value = toml::from_str(config)?;
        Ok(serde_json::to_value(config)?)
    }

    #[test]
    fn test_config_schema() -> Result<()> {
        let schema = config_schema_value()?;

        assert!(jsonschema::draft202012::meta::is_valid(&schema));
        assert_eq!(schema["title"], "Config");
        assert_eq!(schema["additionalProperties"], false);
        assert!(schema.get("required").is_none());
        assert_eq!(schema["properties"]["header"]["default"], json!({}));

        let properties = &schema["properties"];
        assert!(properties["max_retries"].is_object());
        assert!(
            properties["timeout"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("timeout")
        );

        let modes = schema["$defs"]["StatsFormat"]["enum"]
            .as_array()
            .expect("StatsFormat should be represented as an enum");
        assert!(modes.iter().any(|value| value == "json"));

        Ok(())
    }

    #[test]
    fn test_config_schema_accepts_supported_config_forms() -> Result<()> {
        let schema = config_schema_value()?;
        let validator = jsonschema::draft202012::new(&schema)?;
        let configs = [
            ("empty config", ""),
            (
                "example config",
                include_str!("../../../lychee.example.toml"),
            ),
            (
                "custom deserializers",
                r#"
verbose = "Warning"
extensions = ["md", "html"]
cache_exclude_status = [429, "500.."]
archive = "wayback"
accept = 200
method = ["head", "get"]
base_url = "https://example.com"
basic_auth = ["example.com user:password"]
github_token = "secret"
preprocess = { command = "preprocess.sh", ignored = true }

[hosts."example.com"]
concurrency = 2
request_interval = "100ms"
headers = { Accept = "text/html" }
"#,
            ),
        ];

        for (name, config) in configs {
            if let Err(error) = toml::from_str::<Config>(config) {
                bail!("{name} should deserialize as Config: {error}");
            }

            let instance = toml_to_json(config)?;
            if let Err(error) = validator.validate(&instance) {
                bail!("{name} should match the generated schema: {error}");
            }
        }

        Ok(())
    }

    #[test]
    fn test_config_schema_rejects_unsupported_config_forms() -> Result<()> {
        let schema = config_schema_value()?;
        let validator = jsonschema::draft202012::new(&schema)?;
        let configs = [
            ("unknown root option", "unknown = true"),
            (
                "unknown host option",
                "[hosts.\"example.com\"]\nunknown = true",
            ),
            ("unsupported archive", "archive = \"unknown\""),
            ("unsupported verbosity", "verbose = \"loud\""),
            ("empty method list", "method = []"),
            ("empty method string", "method = \"\""),
            ("status below minimum", "accept = 42"),
            ("status string below minimum", "accept = \"42\""),
            ("malformed basic auth", "basic_auth = [\"user:password\"]"),
        ];

        for (name, config) in configs {
            assert!(
                toml::from_str::<Config>(config).is_err(),
                "{name} should not deserialize as Config"
            );

            let instance = toml_to_json(config)?;
            assert!(
                !validator.is_valid(&instance),
                "{name} should not match the generated schema"
            );
        }

        Ok(())
    }

    #[test]
    fn test_man_page() -> Result<()> {
        let roff = man_page()?;

        // Must contain description
        assert!(roff.contains("lychee \\- A fast, async link checker"));
        assert!(roff.contains(
            "lychee is a fast, asynchronous link checker which detects broken URLs and mail addresses in local files and websites. It supports Markdown and HTML and works with other file formats."
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
