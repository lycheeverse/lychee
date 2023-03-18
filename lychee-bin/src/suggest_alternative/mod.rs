use anyhow::anyhow;
use std::str::FromStr;

use http::StatusCode;
use reqwest::{Error, Url};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Serialize, Eq, Hash, PartialEq)]
pub(crate) struct Suggestion {
    pub(crate) url: Url,
    pub(crate) suggestion: Url,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub(crate) enum Archive {
    #[default]
    WaybackMachine,
}

impl FromStr for Archive {
    type Err = anyhow::Error;
    fn from_str(archive: &str) -> Result<Self, Self::Err> {
        match archive.to_lowercase().as_str() {
            "wayback" => Ok(Archive::WaybackMachine),
            _ => Err(anyhow!("Unknown archive {}", archive)),
        }
    }
}

impl Archive {
    pub(crate) async fn get_link(&self, original: &Url) -> Result<Option<Url>, Error> {
        let function = match self {
            Archive::WaybackMachine => get_wayback_link,
        };

        function(original).await
    }
}

async fn get_wayback_link(url: &Url) -> Result<Option<Url>, Error> {
    let mut archive_url = Url::parse("https://archive.org/wayback/available").unwrap();
    archive_url.set_query(Some(&format!("url={}", url)));

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
    let result = value.parse::<u16>().unwrap();
    Ok(StatusCode::from_u16(result).unwrap())
}

#[tokio::test]
async fn wayback_suggestion() -> Result<(), Error> {
    let url = &"https://example.com".try_into().unwrap();
    let response = get_wayback_link(url).await?;
    let suggestion = response.unwrap();

    assert!(suggestion.as_str().contains("web.archive.org"));

    Ok(())
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
