use crate::Status;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
/// A list of URLs that were followed through HTTP redirects,
/// starting from the original URL and ending at the final destination.
/// Each entry in the chain represents a step in the redirect sequence.
pub struct RedirectChain(Vec<Url>);

impl From<Vec<Url>> for RedirectChain {
    fn from(value: Vec<Url>) -> Self {
        Self(value)
    }
}

impl RedirectChain {
    /// Count how many times a redirect was followed.
    /// This is the lenght of the chain minus one
    pub(crate) fn redirect_count(&self) -> usize {
        self.0.len().saturating_sub(1)
    }
}

#[derive(Debug, Clone)]
/// Keep track of HTTP redirections for reporting
pub(crate) struct RedirectTracker(Arc<Mutex<HashMap<Url, RedirectChain>>>);

impl RedirectTracker {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())))
    }

    pub(crate) fn record_redirect(&self, redirect_chain: &[Url]) {
        if let Ok(mut map) = self.0.lock() {
            if let Some(first) = redirect_chain.first() {
                map.insert(first.clone(), RedirectChain(redirect_chain.to_vec()));
            }
        }
    }

    pub(crate) fn handle_redirected(&self, url: &Url, status: Status) -> Status {
        match status {
            Status::Ok(code) => {
                if let Some(chain) = self.get_resolved(url) {
                    Status::Redirected(code, chain)
                } else {
                    Status::Ok(code)
                }
            }
            s => s,
        }
    }

    pub(crate) fn get_resolved(&self, original: &Url) -> Option<RedirectChain> {
        self.0.lock().ok()?.get(original).cloned()
    }
}

impl Default for RedirectTracker {
    fn default() -> Self {
        Self::new()
    }
}
