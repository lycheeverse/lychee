//! A module to generate lychee-bin related output for usability purposes.
//! The generated data is not related to the main use-cases of lychee
//! such as link checking but for usability purposes, such as the manual page
//! and shell completions.

use anyhow::Result;
use clap::CommandFactory;
use serde::Deserialize;
use strum::{Display, EnumIter, EnumString, VariantNames};

use crate::LycheeOptions;

/// What to generate when provided the --generate flag
#[derive(Debug, Deserialize, Clone, Display, EnumIter, EnumString, VariantNames, PartialEq)]
#[non_exhaustive]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub(crate) enum GenerateMode {
    /// Generate roff used for the man page
    Man,
}

pub(crate) fn generate(mode: &GenerateMode) -> Result<String> {
    match mode {
        GenerateMode::Man => man_page(),
    }
}

/// Generate the lychee man page in roff format using [`clap_mangen`]
fn man_page() -> Result<String> {
    let date = chrono::offset::Local::now().format("%Y-%m-%d");
    let man = clap_mangen::Man::new(LycheeOptions::command()).date(format!("{date}"));

    let mut buffer: Vec<u8> = Vec::default();
    man.render(&mut buffer)?;

    Ok(std::str::from_utf8(&buffer)?.to_owned())
}

#[cfg(test)]
mod tests {
    use super::man_page;
    use anyhow::Result;

    #[test]
    fn test_man_pages() -> Result<()> {
        let roff = man_page()?;

        // Must contain description
        assert!(roff.contains("lychee \\- A fast, async link checker"));
        assert!(roff.contains(
            "lychee is a tool to detect broken URLs and mail addresses in local files and websites."
        ));
        assert!(
            roff.contains(
                "lychee is powered by lychee\\-lib, the Rust library to for link checking."
            )
        );

        // Flags should normally occur exactly twice.
        // Once in SYNOPSIS and once in OPTIONS.
        assert_eq!(roff.matches("\\-\\-version").count(), 2);
        Ok(())
    }
}
