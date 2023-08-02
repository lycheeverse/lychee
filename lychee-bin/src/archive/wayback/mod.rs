use once_cell::sync::Lazy;
use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer};

use http::StatusCode;
use reqwest::{Error, Url};
static WAYBACK_URL: Lazy<Url> =
    Lazy::new(|| Url::parse("https://archive.org/wayback/available").unwrap());

pub(crate) async fn get_wayback_link(url: &Url) -> Result<Option<Url>, Error> {
    let mut archive_url: Url = WAYBACK_URL.clone();
    archive_url.set_query(Some(&format!("url={url}")));

    let response = reqwest::get(archive_url)
        .await?
        .json::<InternetArchiveResponse>()
        .await?;

    Ok(response
        .archived_snapshots
        .closest
        .map(|closest| closest.url))
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub(crate) struct InternetArchiveResponse {
    pub(crate) url: Url,
    pub(crate) archived_snapshots: ArchivedSnapshots,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub(crate) struct ArchivedSnapshots {
    pub(crate) closest: Option<Closest>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub(crate) struct Closest {
    #[serde(deserialize_with = "from_string")]
    pub(crate) status: StatusCode,
    pub(crate) available: bool,
    pub(crate) url: Url,
    pub(crate) timestamp: String,
}

fn from_string<'d, D>(deserializer: D) -> Result<StatusCode, D::Error>
where
    D: Deserializer<'d>,
{
    let value: &str = Deserialize::deserialize(deserializer)?;
    let result = value
        .parse::<u16>()
        .map_err(|e| D::Error::custom(e.to_string()))?;
    StatusCode::from_u16(result).map_err(|e| D::Error::custom(e.to_string()))
}

#[cfg(test)]
mod tests {
    use crate::archive::wayback::get_wayback_link;
    use reqwest::Error;
    use std::{error::Error as StdError, time::Duration};
    use tokio::time::sleep;

    #[tokio::test]
    async fn wayback_suggestion() -> Result<(), Box<dyn StdError>> {
        let url = "https://example.com".parse()?;

        // This test can be flaky, because the wayback machine does not always
        // return a suggestion. Retry a few times if needed.
        for _ in 0..3 {
            if let Some(suggestion) = get_wayback_link(&url).await? {
                assert_eq!(
                    suggestion
                        .host_str()
                        .expect("Suggestion doesn't have a host"),
                    "web.archive.org"
                );
                assert!(suggestion.path().ends_with(url.as_str()));
                return Ok(());
            }
            sleep(Duration::from_secs(1)).await; // add delay between retries
        }
        Err("Did not get a valid Wayback Machine suggestion.".into())
    }

    #[tokio::test]
    async fn wayback_suggestion_unknown_url() -> Result<(), Error> {
        let url = &"https://github.com/mre/idiomatic-rust-doesnt-exist-man"
            .try_into()
            .unwrap();

        let response = get_wayback_link(url).await?;
        assert_eq!(response, None);
        Ok(())
    }
}
