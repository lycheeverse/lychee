//! Try to detect if websites employ bot detection mechanisms
//! to inform users and provide a better user experience.

use std::fmt::Display;

use reqwest::Response;

use crate::{Uri, hint};

enum AntiBotSoftware {
    /// <https://datadome.co/>
    Datadome,
    /// <https://developers.cloudflare.com/>
    Cloudflare,
    /// <https://git.gammaspectra.live/git/go-away>
    GoAway,
    /// <https://anubis.techaro.lol/>
    Anubis,
}

impl Display for AntiBotSoftware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            AntiBotSoftware::Datadome => "Datadome",
            AntiBotSoftware::Cloudflare => "Cloudflare",
            AntiBotSoftware::GoAway => "go-away",
            AntiBotSoftware::Anubis => "Anubis",
        };

        write!(f, "{name}")
    }
}

/// Warn users about detected bot protection mechanisms for an improved user experience.
pub(crate) fn hint_bot_detection(uri: &Uri, response: &Response) {
    if let Some(host) = uri.url.host()
        && let Some(anti_bot_software) = detect_software(response)
    {
        hint!("Detected {anti_bot_software} anti-bot protection on website {host}");
    }
}

/// Try to detect if the website empoys any common anti-bot software,
/// affecting the results of lychee in an unexpected way.
/// This can be reported to users for an improved user experience.
///
/// # Returns
///
/// Returns `None` if no common mechanism was detected.
fn detect_software(response: &Response) -> Option<AntiBotSoftware> {
    let is_client_error = response.status().is_client_error();
    let headers = response.headers();

    if is_client_error && headers.get("x-datadome").is_some() {
        // `curl -v https://www.marketwatch.com/story/lychee-is-the-best-link-checker`
        return Some(AntiBotSoftware::Datadome);
    }

    if is_client_error
        && headers
            .get("server")
            .is_some_and(|h| h.to_str().is_ok_and(|v| v == "cloudflare"))
    {
        // `curl -v https://www.winehq.org`
        return Some(AntiBotSoftware::Cloudflare);
    }

    if is_client_error
        && headers
            .get("set-cookie")
            .is_some_and(|h| h.to_str().is_ok_and(|v| v.contains("go-away")))
    {
        // `curl https://www.freedesktop.org/wiki/ --user-agent 'Mozilla/5.0' -v`
        return Some(AntiBotSoftware::GoAway);
    }

    // This unfortunately doesn't seem to be entirely reliable..
    if headers.get("refresh").is_some_and(|h| {
        h.to_str()
            .is_ok_and(|v| v.contains("anubis/api/pass-challenge"))
    }) {
        // `curl -v https://anubis.techaro.lol/`
        return Some(AntiBotSoftware::Anubis);
    }

    None
}
