use reqwest::{Error, Url};
use serde::Deserialize;

pub(crate) async fn get_wayback_link(url: Url) -> Result<InternetArchiveResponse, Error> {
    let mut archive_url = Url::parse("https://archive.org/wayback/available").unwrap();
    archive_url.set_query(Some(&format!("url={}", url)));

    Ok(reqwest::get(archive_url)
        .await?
        .json::<InternetArchiveResponse>()
        .await?)
}

#[derive(Deserialize)]
pub(crate) struct InternetArchiveResponse {
    pub(crate) url: Url,
    pub(crate) archived_snapshots: ArchivedSnapshots,
}

#[derive(Deserialize)]
pub(crate) struct ArchivedSnapshots {
    pub(crate) closest: Option<Closest>,
}

#[derive(Deserialize)]
pub(crate) struct Closest {
    pub(crate) status: String, // todo: use a dedicated status code type or a u16
    pub(crate) available: bool,
    pub(crate) url: Url,
    pub(crate) timestamp: String,
}

#[tokio::test]
async fn valid_wayback_suggestion() -> Result<(), Error> {
    let url = "https://example.com".try_into().unwrap();
    let link = get_wayback_link(url).await?;

    assert_eq!(link.url, "https://example.com".try_into().unwrap());
    assert_eq!(link.archived_snapshots.closest.available, true);
    assert_eq!(link.archived_snapshots.closest.status, "200");
    assert!(link
        .archived_snapshots
        .closest
        .url
        .as_str()
        .contains("web.archive.org"));

    Ok(())
}
