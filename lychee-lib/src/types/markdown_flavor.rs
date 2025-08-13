use serde::Deserialize;
use strum::{Display, EnumString, VariantNames};

/// define the markdown flavor
#[derive(
    Default, Clone, Deserialize, Debug, Copy, Display, EnumString, VariantNames, PartialEq,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MarkdownFlavor {
    /// The default style "classic markdown"
    #[serde(rename = "commonmark")]
    #[strum(serialize = "commonmark", ascii_case_insensitive)]
    #[default]
    CommonMark,

    /// Media Wiki style
    #[serde(rename = "mediawiki")]
    #[strum(serialize = "mediawiki", ascii_case_insensitive)]
    MediaWiki,
}
