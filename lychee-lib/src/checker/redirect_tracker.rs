use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use url::Url;

#[derive(Debug, Clone)]
pub(crate) struct RedirectTracker(Arc<Mutex<HashMap<Url, Vec<Url>>>>);

impl RedirectTracker {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())))
    }

    pub(crate) fn record_redirect(&self, original: Url, previous: Vec<Url>) {
        if let Ok(mut map) = self.0.lock() {
            map.insert(original, previous);
        }
    }

    pub(crate) fn get_resolved(&self, original: &Url) -> Option<Vec<Url>> {
        self.0.lock().ok()?.get(original).cloned().map(|mut l| {
            l.insert(0, original.clone());
            l
        })
    }

    // pub(crate) fn all_redirects(&self) -> HashMap<Url, Url> {
    //     self.0.lock().unwrap_or_else(|p| p.into_inner()).clone() // ignore poisoning
    // }
}

impl Default for RedirectTracker {
    fn default() -> Self {
        Self::new()
    }
}
