use reqwest::{Error, Url};
use serde::Deserialize;
use std::time::Duration;
use strum::{Display, EnumIter, EnumString, VariantNames};

mod wayback;

#[non_exhaustive]
#[derive(Debug, Deserialize, Default, Clone, Display, EnumIter, EnumString, VariantNames)]
/// The different supported online archive sites for restoring broken links.
pub enum Archive {
    #[serde(rename = "wayback")]
    #[strum(serialize = "wayback", ascii_case_insensitive)]
    #[default]
    /// The most prominent digital archive provided by the Interne Archive (https://archive.org)
    WaybackMachine,
}

impl Archive {
    /// Query the `Archive` to try and find the latest snapshot of the specified `url`.
    /// Returns `None` if the specified `url` hasn't been archived in the past.
    pub async fn get_snapshot(&self, url: &Url, timeout: Duration) -> Result<Option<Url>, Error> {
        let function = match self {
            Archive::WaybackMachine => wayback::get_wayback_link,
        };

        function(url, timeout).await
    }
}
