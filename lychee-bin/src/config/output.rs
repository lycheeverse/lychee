//! Output configuration and formats.
//!
//! Defines the supported output formatting options for the CLI,
//! such as color modes and output formats (e.g. JSON, Compact, Detailed).

use anyhow::{Error, Result, anyhow};
use serde::Deserialize;
use std::str::FromStr;
use strum::{Display, EnumIter, EnumString, VariantNames};

/// The format to use for the final status report
#[derive(Debug, Deserialize, Default, Clone, Display, EnumIter, VariantNames, PartialEq)]
#[non_exhaustive]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub(crate) enum StatsFormat {
    #[default]
    Compact,
    Detailed,
    Json,
    Junit,
    Markdown,
}

impl FromStr for StatsFormat {
    type Err = Error;

    fn from_str(format: &str) -> Result<Self, Self::Err> {
        match format.to_lowercase().as_str() {
            "compact" | "string" => Ok(StatsFormat::Compact),
            "detailed" => Ok(StatsFormat::Detailed),
            "json" => Ok(StatsFormat::Json),
            "junit" => Ok(StatsFormat::Junit),
            "markdown" | "md" => Ok(StatsFormat::Markdown),
            _ => Err(anyhow!("Unknown format {format}")),
        }
    }
}

/// The different formatter modes
///
/// This decides over whether to use color,
/// emojis, or plain text for the output.
#[derive(
    Debug, Deserialize, Default, Clone, Display, EnumIter, EnumString, VariantNames, PartialEq,
)]
#[non_exhaustive]
pub(crate) enum OutputMode {
    /// Plain text output.
    ///
    /// This is the most basic output mode for terminals that do not support
    /// color or emojis. It can also be helpful for scripting or when you want
    /// to pipe the output to another program.
    #[serde(rename = "plain")]
    #[strum(serialize = "plain", ascii_case_insensitive)]
    Plain,

    /// Colorful output.
    ///
    /// This mode uses colors to highlight the status of the requests.
    /// It is useful for terminals that support colors and you want to
    /// provide a more visually appealing output.
    ///
    /// This is the default output mode.
    #[serde(rename = "color")]
    #[strum(serialize = "color", ascii_case_insensitive)]
    #[default]
    Color,

    /// Emoji output.
    ///
    /// This mode uses emojis to represent the status of the requests.
    /// Some people may find this mode more intuitive and fun to use.
    #[serde(rename = "emoji")]
    #[strum(serialize = "emoji", ascii_case_insensitive)]
    Emoji,

    /// Task output.
    ///
    /// This mode uses Markdown-styled checkboxes to represent the status of the requests.
    /// Some people may find this mode more intuitive and useful for task tracking.
    #[serde(rename = "task")]
    #[strum(serialize = "task", ascii_case_insensitive)]
    Task,
}

impl OutputMode {
    /// Returns `true` if the response format is `Plain`
    pub(crate) const fn is_plain(&self) -> bool {
        matches!(self, OutputMode::Plain)
    }

    /// Returns `true` if the response format is `Emoji`
    pub(crate) const fn is_emoji(&self) -> bool {
        matches!(self, OutputMode::Emoji)
    }
}
