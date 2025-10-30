use crate::Status;
use serde::Serialize;
use std::fmt::Display;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use url::Url;

/// A list of URLs that were followed through HTTP redirects,
/// starting from the original URL and ending at the final destination.
/// Each entry in the list represents a step in the redirect sequence.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Redirects(Vec<Url>);

impl From<Vec<Url>> for Redirects {
    fn from(value: Vec<Url>) -> Self {
        Self(value)
    }
}

impl Display for Redirects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list = self
            .0
            .iter()
            .map(Url::as_str)
            .collect::<Vec<_>>()
            .join(" --> ");
        write!(f, "{list}")
    }
}

impl Redirects {
    /// Count how many times a redirect was followed.
    /// This is the length of the list minus one.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.0.len().saturating_sub(1)
    }

    /// Represents zero redirects
    #[must_use]
    pub const fn none() -> Self {
        Redirects(vec![])
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
    pub(crate) fn record_redirects(&self, redirects: &[Url]) {
        if let (Ok(mut map), Some(first)) = (self.0.lock(), redirects.first()) {
            map.insert(first.clone(), Redirects(redirects.to_vec()));
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
