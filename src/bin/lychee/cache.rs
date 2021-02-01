use std::collections::HashMap;

use lychee::Uri;

/// Link cache for recursion and to avoid checking a link multiple times
pub struct Cache {
    pub cache: HashMap<Uri, usize>,
}

impl Cache {
    pub fn new() -> Self {
        let cache = HashMap::new();
        Cache { cache }
    }

    pub fn add(&mut self, uri: Uri) {
        *self.cache.entry(uri).or_insert(0) += 1;
    }

    pub fn contains(&self, uri: &Uri) -> bool {
        self.cache.contains_key(uri)
    }
}
