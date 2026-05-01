//! Wayback Machine integration for suggesting archived versions of dead pages.
//!
//! Builds URLs of the form:
//!
//! ```text
//! https://web.archive.org/web/0/<url>
//! ```
//!
//! Using `0` as the timestamp tells Wayback "give me the best (newest)
//! available snapshot". The server's response tells us whether a snapshot
//! exists:
//!
//! - **`302 Found`** with a `Location:` header pointing at the actual
//!   timestamped snapshot (e.g. `/web/20020120142510/http://example.com/`).
//!   We follow the redirect and surface the resolved URL. That way users
//!   see the capture date in the suggested link.
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
use reqwest::{Client, Error, Url};

/// Per-request timeout for Wayback lookups.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Shared HTTP client for all Wayback suggestion lookups.
///
/// The suggestion path may issue dozens of lookups in quick succession
/// (one per failed URL), all aimed at the same host. Sharing a client
/// lets them reuse the connection pool, TLS session cache, and DNS
/// resolver instead of paying those costs per request.
static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("Wayback HTTP client should always build with default config")
});

/// Construct a Wayback Machine URL pointing at the best available snapshot
/// of `url`, or `None` if Wayback has no snapshot of the page.
///
/// Performs a single GET request against `https://web.archive.org/web/0/<url>`
/// (following redirects).
///
/// Wayback resolves the `0` timestamp server-side and either redirects to the
/// latest snapshot or responds `404`.
pub(crate) async fn get_archive_snapshot(url: &Url) -> Result<Option<Url>, Error> {
    let wayback = format!("https://web.archive.org/web/0/{url}");
    // `Url::parse` cannot fail here for any well-formed input `url`, but
    // if it ever did we'd rather report "no suggestion" than panic.
    let Ok(snapshot_url) = Url::parse(&wayback) else {
        log::debug!("failed to construct Wayback URL for {url}");
        return Ok(None);
    };

    let response = CLIENT.get(snapshot_url).send().await?;

    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }

    // Any other non-success (rate limiting, 5xx, ...) bubbles up so the
    // caller can decide; the suggestion path currently swallows errors.
    let response = response.error_for_status()?;

    // After redirect-following, `response.url()` is the resolved
    // timestamped snapshot URL (e.g. `/web/20020120142510/http://...`).
    Ok(Some(response.url().clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as StdError;

    #[tokio::test]
    /// Real Wayback request: `example.com` is archived many times over,
    /// so this should resolve to a concrete timestamped snapshot URL.
    ///
    /// Tolerates 503s, which `web.archive.org` returns intermittently
    /// under load.
    async fn wayback_suggestion_real() -> Result<(), Box<dyn StdError>> {
        let url: Url = "https://example.com".parse()?;
        match get_archive_snapshot(&url).await {
            Ok(snapshot) => {
                let snapshot = snapshot.expect("example.com should have a snapshot");
                let s = snapshot.as_str();
                assert!(
                    s.starts_with("https://web.archive.org/web/"),
                    "unexpected snapshot URL: {s}"
                );
                // Resolved snapshots embed a 14-digit timestamp, never
                // the `0` sentinel. If we ever see `/web/0/` here,
                // redirect-following silently broke.
                assert!(!s.contains("/web/0/"), "redirect was not followed: {s}");
            }
            Err(e) if e.status() == Some(StatusCode::SERVICE_UNAVAILABLE) => {
                // ignore 503s which are *probably* transient
            }
            Err(e) => Err(e)?,
        }
        Ok(())
    }

    #[tokio::test]
    /// Real Wayback request for a URL that doesn't exist (and therefore
    /// can't have been archived). Wayback responds 404 and we must
    /// return `None` so lychee doesn't suggest a dead-end link.
    async fn wayback_suggestion_real_unknown() -> Result<(), Box<dyn StdError>> {
        let url: Url = "https://example.com/this-page-does-not-exist-abc123xyz".parse()?;
        match get_archive_snapshot(&url).await {
            Ok(snapshot) => assert_eq!(snapshot, None),
            Err(e) if e.status() == Some(StatusCode::SERVICE_UNAVAILABLE) => {
                // ignore 503s which are *probably* transient
            }
            Err(e) => Err(e)?,
        }
        Ok(())
    }
}
