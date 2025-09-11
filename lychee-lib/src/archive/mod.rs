use reqwest::{Error, Url};
use serde::Deserialize;
use std::time::Duration;
use strum::{Display, EnumIter, EnumString, VariantNames};

mod wayback;

/// The different supported online archive sites for restoring broken links.
#[non_exhaustive]
#[derive(
    Debug, Deserialize, Default, Clone, Display, EnumIter, EnumString, VariantNames, PartialEq, Eq,
)]
pub enum Archive {
    #[serde(rename = "wayback")]
    #[strum(serialize = "wayback", ascii_case_insensitive)]
    #[default]
    /// The most prominent digital archive provided by the [Interne Archive](https://archive.org)
    WaybackMachine,
}

impl Archive {
    /// Query the `Archive` to try and find the latest snapshot of the specified `url`.
    /// Returns `None` if the specified `url` hasn't been archived in the past.
    ///
    /// # Errors
    ///
    /// Returns an error if the `reqwest` client cannot be built, the request itself fails
    /// or the API response cannot be parsed.
    pub async fn get_archive_snapshot(
        &self,
        url: &Url,
        timeout: Duration,
    ) -> Result<Option<Url>, Error> {
        let function = match self {
            Archive::WaybackMachine => wayback::get_archive_snapshot,
        };

        function(url, timeout).await
    }
}
