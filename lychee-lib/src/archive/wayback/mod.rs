use std::sync::LazyLock;
use std::time::Duration;

use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer};

use http::StatusCode;
use reqwest::{Client, Error, Url};

static WAYBACK_URL: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://archive.org/wayback/available").unwrap());

pub(crate) async fn get_archive_snapshot(
    url: &Url,
    timeout: Duration,
) -> Result<Option<Url>, Error> {
    get_archive_snapshot_internal(url, timeout, WAYBACK_URL.clone()).await
}

async fn get_archive_snapshot_internal(
    url: &Url,
    timeout: Duration,
    mut api: Url,
) -> Result<Option<Url>, Error> {
    let url = url.to_string();

    // The Wayback API doesn't return any snapshots for URLs with trailing slashes
    let stripped = url.strip_suffix("/").unwrap_or(&url);
    api.set_query(Some(&format!("url={stripped}")));

    let response = Client::builder()
        .timeout(timeout)
        .build()?
        .get(api)
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
    use crate::archive::wayback::{get_archive_snapshot, get_archive_snapshot_internal};
    use http::StatusCode;
    use reqwest::{Client, Error, Url};
    use std::{error::Error as StdError, time::Duration};
    use wiremock::matchers::query_param;

    const TIMEOUT: Duration = Duration::from_secs(20);

    #[tokio::test]
    /// Test retrieval by mocking the Wayback API.
    /// We mock their API because unfortunately it happens quite often that the
    /// `archived_snapshots` field is empty because the API is unreliable.
    /// This way we avoid flaky tests.
    async fn wayback_suggestion_mocked() -> Result<(), Box<dyn StdError>> {
        let mock_server = wiremock::MockServer::start().await;
        let api_url = mock_server.uri();
        let api_response = wiremock::ResponseTemplate::new(StatusCode::OK).set_body_raw(
            r#"
                {
                    "url": "https://google.com/jobs.html",
                    "archived_snapshots": {
                        "closest": {
                            "available": true,
                            "url": "http://web.archive.org/web/20130919044612/http://example.com/",
                            "timestamp": "20130919044612",
                            "status": "200"
                        }
                    }
                }
                "#,
            "application/json",
        );

        let url_to_restore = "https://example.com".parse::<Url>()?;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(query_param(
                "url",
                url_to_restore.as_str().strip_suffix("/").unwrap(),
            ))
            .respond_with(api_response)
            .mount(&mock_server)
            .await;

        let result =
            get_archive_snapshot_internal(&url_to_restore, TIMEOUT, api_url.parse()?).await;

        assert_eq!(
            result?,
            Some("http://web.archive.org/web/20130919044612/http://example.com/".parse()?)
        );

        Ok(())
    }

    #[tokio::test]
    /// Their API documentation mentions when the last changes occurred.
    /// Because we mock their API in previous tests we try to detect breaking API changes with this test.
    async fn wayback_api_no_breaking_changes() -> Result<(), Error> {
        let api_docs_url = "https://archive.org/help/wayback_api.php";
        let html = Client::builder()
            .timeout(TIMEOUT)
            .build()?
            .get(api_docs_url)
            .send()
            .await?
            .text()
            .await?;

        assert!(html.contains("Updated on September, 24, 2013"));
        Ok(())
    }

    #[ignore = "
        It is flaky because the API does not reliably return snapshots,
        i.e. the `archived_snapshots` field is unreliable.
        That's why the test is ignored. For development and documentation this test is still useful."]
    #[tokio::test]
    /// This tests the real Wayback API without any mocks.
    async fn wayback_suggestion_real() -> Result<(), Box<dyn StdError>> {
        let url = &"https://example.com".try_into()?;
        let response = get_archive_snapshot(url, TIMEOUT).await?;
        assert_eq!(
            response,
            Some("http://web.archive.org/web/20250603204626/http://www.example.com/".parse()?)
        );
        Ok(())
    }

    #[tokio::test]
    /// This tests the real Wayback API without any mocks.
    /// The flakiness of the API shouldn't affect this test because it originates from
    /// the `archived_snapshots` field.
    async fn wayback_suggestion_real_unknown() -> Result<(), Box<dyn StdError>> {
        let url = &"https://github.com/mre/idiomatic-rust-doesnt-exist-man".try_into()?;
        let response = get_archive_snapshot(url, TIMEOUT).await?;
        assert_eq!(response, None);
        Ok(())
    }
}
