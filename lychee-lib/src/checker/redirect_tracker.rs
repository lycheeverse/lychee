use crate::Status;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use url::Url;

#[derive(Debug, Clone)]
/// Keep track of HTTP redirections for reporting
pub(crate) struct RedirectTracker(Arc<Mutex<HashMap<Url, Vec<Url>>>>);

impl RedirectTracker {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())))
    }

    pub(crate) fn record_redirect(&self, redirect_chain: &[Url]) {
        if let Ok(mut map) = self.0.lock() {
            if let Some((first, rest)) = redirect_chain.split_first() {
                map.insert(first.clone(), rest.to_vec());
            }
        }
    }

    pub(crate) fn handle_redirected(&self, url: &Url, status: Status) -> Status {
        match status {
            Status::Ok(status_code) => {
                if let Some(redirect_chain) = self.get_resolved(url) {
                    dbg!(redirect_chain);
                    // TODO: Status::Redirected(Status, RedirectChain)
                    Status::Redirected(status_code)
                } else {
                    Status::Ok(status_code)
                }
            }
            s => s,
        }
    }

    pub(crate) fn get_resolved(&self, original: &Url) -> Option<Vec<Url>> {
        self.0.lock().ok()?.get(original).cloned()
    }
}

impl Default for RedirectTracker {
    fn default() -> Self {
        Self::new()
    }
}
