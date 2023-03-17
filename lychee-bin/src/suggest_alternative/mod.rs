use http::StatusCode;
use reqwest::{Error, Url};
use serde::{Deserialize, Deserializer};

pub(crate) async fn get_wayback_link(url: &Url) -> Result<InternetArchiveResponse, Error> {
    let mut archive_url = Url::parse("https://archive.org/wayback/available").unwrap();
    archive_url.set_query(Some(&format!("url={}", url)));

    Ok(reqwest::get(archive_url)
        .await?
        .json::<InternetArchiveResponse>()
        .await?)
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
    let closest_snapsnot = response.archived_snapshots.closest.unwrap();

    assert_eq!(&response.url, url);
    assert_eq!(closest_snapsnot.available, true);
    assert_eq!(closest_snapsnot.status, StatusCode::OK);
    assert!(closest_snapsnot.url.as_str().contains("web.archive.org"));

    Ok(())
}

#[tokio::test]
async fn wayback_suggestion_unknown_url() -> Result<(), Error> {
    let url = &"https://github.com/mre/idiomatic-rust-doesnt-exist-man"
        .try_into()
        .unwrap();
    let response = get_wayback_link(url).await?;

    assert_eq!(&response.url, url);
    assert_eq!(response.archived_snapshots.closest, None);
    Ok(())
}
