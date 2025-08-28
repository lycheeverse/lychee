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
}

impl Default for RedirectTracker {
    fn default() -> Self {
        Self::new()
    }
}
