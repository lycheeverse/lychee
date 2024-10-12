use reqwest::{Error, Url};
use serde::{Deserialize, Serialize};
use std::{fmt::Display, time::Duration};
use strum::{Display, EnumIter, EnumString, VariantNames};

use crate::color::{color, GREEN, PINK};

mod wayback;

#[derive(Debug, Serialize, Eq, Hash, PartialEq)]
pub(crate) struct Suggestion {
    pub(crate) original: Url,
    pub(crate) suggestion: Url,
}

impl Display for Suggestion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        color!(f, PINK, "{}", self.original)?;
        write!(f, " ")?;
        color!(f, GREEN, "{}", self.suggestion)?;
        Ok(())
    }
}

#[non_exhaustive]
#[derive(Debug, Deserialize, Default, Clone, Display, EnumIter, EnumString, VariantNames)]
pub(crate) enum Archive {
    #[serde(rename = "wayback")]
    #[strum(serialize = "wayback", ascii_case_insensitive)]
    #[default]
    WaybackMachine,
}

impl Archive {
    pub(crate) async fn get_link(
        &self,
        original: &Url,
        timeout: Duration,
    ) -> Result<Option<Url>, Error> {
        let function = match self {
            Archive::WaybackMachine => wayback::get_wayback_link,
        };

        function(original, timeout).await
    }
}
