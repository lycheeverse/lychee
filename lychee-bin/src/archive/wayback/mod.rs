use std::time::Duration;

use once_cell::sync::Lazy;
use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer};

use http::StatusCode;
use reqwest::{Client, Error, Url};
static WAYBACK_URL: Lazy<Url> =
    Lazy::new(|| Url::parse("https://archive.org/wayback/available").unwrap());

pub(crate) async fn get_wayback_link(url: &Url, timeout: Duration) -> Result<Option<Url>, Error> {
    let mut archive_url: Url = WAYBACK_URL.clone();
    archive_url.set_query(Some(&format!("url={url}")));

    let response = Client::builder()
        .timeout(timeout)
        .build()?
        .get(archive_url)
        .send()
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
    use reqwest::{Error, Url};
    use std::{error::Error as StdError, time::Duration};
    use tokio::time::sleep;

    // This test is currently ignored because it is flaky.
    // The Wayback Machine does not always return a suggestion.
    // We can consider mocking the endpoint in the future.
    #[tokio::test]
    #[ignore = "Wayback Machine currently has certificate issues"]
    async fn wayback_suggestion() -> Result<(), Box<dyn StdError>> {
        let target_url = "https://example.com".parse::<Url>()?;

        // Extract domain from target_url without the scheme and trailing slash
        let expected_ending = (target_url.host_str().ok_or("Invalid target URL")?).to_string();

        // This test can be flaky, because the wayback machine does not always
        // return a suggestion. Retry a few times if needed.
        for _ in 0..3 {
            match get_wayback_link(&target_url, Duration::from_secs(20)).await {
                Ok(Some(suggested_url)) => {
                    // Ensure the host is correct
                    let host = suggested_url
                        .host_str()
                        .ok_or("Suggestion doesn't have a host")?;
                    assert_eq!(host, "web.archive.org");

                    // Extract the actual archived URL from the Wayback URL
                    let archived_url = suggested_url
                        .path()
                        .trim_start_matches("/web/")
                        .split_once('/')
                        .map(|x| x.1)
                        .ok_or("Failed to extract archived URL from Wayback suggestion")?;

                    // Check the ending of the suggested URL without considering trailing slash
                    if !archived_url
                        .trim_end_matches('/')
                        .ends_with(&expected_ending)
                    {
                        return Err(format!(
                            "Expected suggestion '{archived_url}' to end with '{expected_ending}'"
                        )
                        .into());
                    }

                    return Ok(());
                }
                Ok(None) => {
                    // No suggestion was returned, wait and retry
                    sleep(Duration::from_secs(1)).await;
                }
                Err(e) => {
                    // Propagate other errors
                    return Err(format!("Error retrieving Wayback link: {e}").into());
                }
            }
        }

        Err("Did not get a valid Wayback Machine suggestion after multiple attempts.".into())
    }

    #[tokio::test]
    #[ignore = "Wayback Machine currently has certificate issues"]
    async fn wayback_suggestion_unknown_url() -> Result<(), Error> {
        let url = &"https://github.com/mre/idiomatic-rust-doesnt-exist-man"
            .try_into()
            .unwrap();

        let response = get_wayback_link(url, Duration::from_secs(20)).await?;
        assert_eq!(response, None);
        Ok(())
    }
}
