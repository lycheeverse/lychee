//! Wayback Machine integration for suggesting archived versions of dead pages.
//!
//! Builds URLs of the form:
//!
//! ```text
//! https://web.archive.org/web/<url>
//! ```
//!
//! Omitting the timestamp tells Wayback "give me the latest available
//! snapshot". The server's response tells us whether a snapshot exists:
//!
//! - **`302 Found`** with a `Location:` header pointing at the actual
//!   timestamped snapshot (e.g. `/web/20020120142510/http://example.com/`).
//!   We read that header directly, without following the redirect, so users
//!   see the capture date in the suggested link without paying to download
//!   the archived page.
//! - **`404 Not Found`** when no snapshot exists for that URL. We return
//!   `None` so lychee won't suggest a dead-end "page not archived" link.
//!
//! This is far more reliable than the Availability JSON API
//! (`https://archive.org/wayback/available`), which is heavily rate-limited,
//! flaky, and frequently returns empty `archived_snapshots` for pages that are
//! clearly archived.
//!
//! See <https://en.wikipedia.org/wiki/Help:Using_the_Wayback_Machine>.

use std::sync::LazyLock;
use std::time::Duration;

use http::StatusCode;
use reqwest::header::LOCATION;
use reqwest::redirect::Policy;
use reqwest::{Client, Error, Url};

/// Base URL for Wayback Machine snapshot lookups.
///
/// Appending a URL (and omitting a timestamp) tells Wayback to resolve the
/// latest available snapshot of that page.
const WAYBACK_BASE_URL: &str = "https://web.archive.org/web/";

/// Shared HTTP client for all Wayback suggestion lookups.
///
/// The suggestion path may issue dozens of lookups in quick succession
/// (one per failed URL), all aimed at the same host. Sharing a client
/// lets them reuse the connection pool, TLS session cache, and DNS
/// resolver instead of paying those costs per request.
static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        // Wayback's `302` response directly gives
        // us the snapshot URL in the `Location`
        .redirect(Policy::none())
        .build()
        .expect("Wayback HTTP client should always build with default config")
});

/// Construct a Wayback Machine URL pointing at the latest available snapshot
/// of `url`, or `None` if Wayback has no snapshot of the page.
///
/// Performs a single GET request against `https://web.archive.org/web/<url>`
/// and inspects the response:
///
/// - a redirect (`302`) carries the timestamped snapshot in its `Location`
///   header, which we return without following, and
/// - a `404` means there is no snapshot, so we return `None`.
pub(crate) async fn get_archive_snapshot(
    url: &Url,
    timeout: Duration,
) -> Result<Option<Url>, Error> {
    let wayback = format!("{WAYBACK_BASE_URL}{url}");
    // `Url::parse` cannot fail here for any well-formed input `url`, but
    // if it ever did we'd rather report "no suggestion" than panic.
    let Ok(snapshot_url) = Url::parse(&wayback) else {
        log::debug!("failed to construct Wayback URL for {url}");
        return Ok(None);
    };

    let response = CLIENT.get(snapshot_url).timeout(timeout).send().await?;
    let status = response.status();

    if status == StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if status.is_redirection() {
        // Read the snapshot straight from the `Location` header rather than
        // following the redirect. We only need the URL, not the archived
        // page body, and skipping the second request avoids downloading it.
        //
        // The header holds the resolved timestamped snapshot URL
        // (e.g. `/web/20020120142510/http://example.com/`). It may be
        // relative, so resolve it against the request URL.
        let snapshot = response
            .headers()
            .get(LOCATION)
            // HTTP header values aren't guaranteed to be UTF-8.
            // If string conversion fails, it gets treated as "no snapshot."
            .and_then(|location| location.to_str().ok())
            .and_then(|location| response.url().join(location).ok());
        return Ok(snapshot);
    }

    // Any other non-success (rate limiting, 5xx, ...) bubbles up as an `Err`
    // so the caller can log it and explain why a suggestion is missing.
    // Anything else (an unexpected `2xx`) means no usable snapshot.
    response.error_for_status()?;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use http::StatusCode;
    use std::error::Error;
    use std::time::Duration;
    use url::Url;

    use super::{WAYBACK_BASE_URL, get_archive_snapshot};

    const TEST_TIMEOUT: Duration = Duration::from_secs(20);

    /// Both cases run in a single test (and therefore a single Tokio runtime)
    /// on purpose: `get_archive_snapshot` uses a process-wide static
    /// `reqwest::Client` whose connection pool is bound to the runtime that
    /// first initializes it. Splitting these into two `#[tokio::test]`s would
    /// give each its own runtime and let one reuse a pooled connection whose
    /// runtime has already been torn down, failing with `DispatchGone`.
    ///
    /// This hits the live Wayback Machine on purpose: it's our only guard
    /// against the upstream API changing its redirect/`404` behavior in a way
    /// that silently breaks suggestions. To keep it from failing CI on
    /// transient hiccups, intermittent `503`s are tolerated below.
    #[tokio::test]
    async fn wayback_suggestion_real() -> Result<(), Box<dyn Error>> {
        // Real Wayback request: `example.com` is archived many times over,
        // so this should resolve to a concrete timestamped snapshot URL.
        let url: Url = "https://example.com".parse()?;
        match get_archive_snapshot(&url, TEST_TIMEOUT).await {
            Ok(snapshot) => {
                let snapshot = snapshot.expect("example.com should have a snapshot");
                let s = snapshot.as_str();
                let after = s
                    .strip_prefix(WAYBACK_BASE_URL)
                    .unwrap_or_else(|| panic!("unexpected snapshot URL: {s}"));
                // A resolved snapshot embeds a numeric timestamp
                // (e.g. `/web/20020120142510/...`). If we got the bare,
                // un-timestamped request URL back, reading `Location`
                // silently broke.
                let timestamp: String = after.chars().take_while(char::is_ascii_digit).collect();
                assert!(
                    !timestamp.is_empty(),
                    "expected a timestamped snapshot, got: {s}"
                );
            }
            // Ignore 503s, which `web.archive.org` returns intermittently
            // under load.
            Err(e) if e.status() == Some(StatusCode::SERVICE_UNAVAILABLE) => {}
            Err(e) => Err(e)?,
        }

        // Real Wayback request for a URL that doesn't exist (and therefore
        // can't have been archived). Wayback responds 404 and we must
        // return `None` so lychee doesn't suggest a dead-end link.
        let url: Url = "https://example.com/this-page-does-not-exist-abc123xyz".parse()?;
        match get_archive_snapshot(&url, TEST_TIMEOUT).await {
            Ok(snapshot) => assert_eq!(snapshot, None),
            Err(e) if e.status() == Some(StatusCode::SERVICE_UNAVAILABLE) => {}
            Err(e) => Err(e)?,
        }

        Ok(())
    }
}
