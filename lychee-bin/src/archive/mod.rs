use reqwest::{Error, Url};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString, EnumVariantNames};

mod wayback;

#[derive(Debug, Serialize, Eq, Hash, PartialEq)]
pub(crate) struct Suggestion {
    pub(crate) original: Url,
    pub(crate) suggestion: Url,
}

#[non_exhaustive]
#[derive(Debug, Deserialize, Default, Clone, Display, EnumIter, EnumString, EnumVariantNames)]
pub(crate) enum Archive {
    #[serde(rename = "wayback")]
    #[strum(serialize = "wayback", ascii_case_insensitive)]
    #[default]
    WaybackMachine,
}

impl Archive {
    pub(crate) async fn get_link(&self, original: &Url) -> Result<Option<Url>, Error> {
        let function = match self {
            Archive::WaybackMachine => wayback::get_wayback_link,
        };

        function(original).await
    }
}
