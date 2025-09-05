use crate::Status;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Default, Serialize)]
/// A list of URLs that were followed through HTTP redirects,
/// starting from the original URL and ending at the final destination.
/// Each entry in the list represents a step in the redirect sequence.
pub struct Redirects(Vec<Url>);

impl From<Vec<Url>> for Redirects {
    fn from(value: Vec<Url>) -> Self {
        Self(value)
    }
}

impl Redirects {
    /// Count how many times a redirect was followed.
    /// This is the length of the list minus one.
    pub(crate) fn count(&self) -> usize {
        self.0.len().saturating_sub(1)
    }
}

#[derive(Debug, Clone)]
/// Keep track of HTTP redirections for reporting
pub(crate) struct RedirectTracker(Arc<Mutex<HashMap<Url, Redirects>>>);

impl RedirectTracker {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())))
    }

    pub(crate) fn record_redirects(&self, redirects: &[Url]) {
        if let Ok(mut map) = self.0.lock() {
            if let Some(first) = redirects.first() {
                map.insert(first.clone(), Redirects(redirects.to_vec()));
            }
        }
    }

    pub(crate) fn handle_redirected(&self, url: &Url, status: Status) -> Status {
        match status {
            Status::Ok(code) => {
                if let Some(redirects) = self.get_resolved(url) {
                    Status::Redirected(code, redirects)
                } else {
                    Status::Ok(code)
                }
            }
            s => s,
        }
    }

    pub(crate) fn get_resolved(&self, original: &Url) -> Option<Redirects> {
        self.0.lock().ok()?.get(original).cloned()
    }
}

impl Default for RedirectTracker {
    fn default() -> Self {
        Self::new()
    }
}
