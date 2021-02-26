use std::collections::HashSet;

use lychee::Uri;

/// Link cache for recursion and to avoid checking a link multiple times
#[derive(Debug)]
pub struct Cache {
    pub cache: HashSet<String>,
}

impl Cache {
    pub fn new() -> Self {
        let cache = HashSet::new();
        Cache { cache }
    }

    pub fn add(&mut self, uri: String) {
        self.cache.insert(uri);
    }

    pub fn contains(&self, uri: String) -> bool {
        self.cache.contains(&uri)
    }
}
