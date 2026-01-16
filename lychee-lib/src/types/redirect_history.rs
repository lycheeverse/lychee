use crate::Status;
use crate::types::cache::serialize_status_code;
use http::StatusCode;
use reqwest::redirect::Attempt;
use serde::Serialize;
use std::fmt::Display;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
/// Represents a single HTTP redirection
pub struct Redirect {
    /// Where we got redirected to
    pub url: Url,
    /// With what status code we got redirected
    #[serde(serialize_with = "serialize_status_code")]
    #[serde(flatten)]
    pub code: StatusCode,
}

/// A list of URLs that were followed through HTTP redirects,
/// starting from the original URL and ending at the final destination.
/// Each entry in the list represents a step in the redirect sequence.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Redirects {
    /// Initial URL from which redirect resolution begins
    origin: Url,
    /// Ordered list of [`Redirect`]s encountered while resolving the request
    redirects: Vec<Redirect>,
}

impl Display for Redirects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut list = self.origin.to_string();
        for redirect in &self.redirects {
            list += &format!(" --[{}]--> {}", redirect.code.as_u16(), redirect.url).to_string();
        }

        write!(f, "{list}")
    }
}

impl Redirects {
    /// Create a new [`Redirects`] instance
    /// where no redirects are yet recorded.
    #[must_use]
    pub const fn new(origin: Url) -> Self {
        Self {
            origin,
            redirects: vec![],
        }
    }

    /// Count how many times a redirect was followed.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.redirects.len()
    }

    /// Record a new redirect
    pub fn push(&mut self, redirect: Redirect) {
        self.redirects.push(redirect);
    }
}

/// Keep track of HTTP redirections for reporting
#[derive(Debug, Clone)]
pub(crate) struct RedirectHistory(Arc<Mutex<HashMap<Url, Redirects>>>);

impl RedirectHistory {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())))
    }

    /// Records a redirect chain, using the original URL as the key.
    ///
    /// The first URL in the chain is treated as the original request URL,
    /// and the entire chain (including the original) is stored as the value.
    /// This allows later lookups of redirect paths by the initial URL.
    pub(crate) fn record_redirects(&self, attempt: &Attempt) {
        let mut map = self.0.lock().unwrap();
        if let Some(first) = attempt.previous().first().cloned() {
            let mut redirects = map.remove(&first).unwrap_or(Redirects::new(first.clone()));

            redirects.push(Redirect {
                url: attempt.url().clone(),
                code: attempt.status(),
            });

            map.insert(first, redirects);
        }
    }

    pub(crate) fn handle_redirected(&self, url: &Url, status: Status) -> Status {
        match status {
            Status::Ok(code) => self
                .get_resolved(url)
                .map(|redirects| Status::Redirected(code, redirects))
                .unwrap_or(Status::Ok(code)),
            other => other,
        }
    }

    fn get_resolved(&self, original: &Url) -> Option<Redirects> {
        self.0.lock().ok()?.get(original).cloned()
    }
}

impl Default for RedirectHistory {
    fn default() -> Self {
        Self::new()
    }
}
