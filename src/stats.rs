use std::{
    collections::HashSet,
    fmt::{self, Display},
};

use crate::types::Response;
use crate::types::Status::*;
use crate::types::Uri;

pub struct ResponseStats {
    total: usize,
    successful: usize,
    failures: HashSet<Uri>,
    timeouts: HashSet<Uri>,
    redirects: HashSet<Uri>,
    excludes: HashSet<Uri>,
    errors: HashSet<Uri>,
}

impl ResponseStats {
    pub fn new() -> Self {
        ResponseStats {
            total: 0,
            successful: 0,
            failures: HashSet::new(),
            timeouts: HashSet::new(),
            redirects: HashSet::new(),
            excludes: HashSet::new(),
            errors: HashSet::new(),
        }
    }

    pub fn add(&mut self, response: Response) {
        self.total += 1;
        let uri = response.uri;
        if !match response.status {
            Failed(_) => self.failures.insert(uri),
            Timeout => self.timeouts.insert(uri),
            Redirected => self.redirects.insert(uri),
            Excluded => self.excludes.insert(uri),
            Error(_) => self.errors.insert(uri),
            _ => false,
        } {
            self.successful += 1;
        }
    }

    pub fn is_success(&self) -> bool {
        self.total == self.successful + self.excludes.len()
    }
}

impl Display for ResponseStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "📝 Summary")?;
        writeln!(f, "-------------------")?;
        writeln!(f, "🔍 Total: {}", self.total)?;
        writeln!(f, "✅ Successful: {}", self.successful)?;
        writeln!(f, "⏳ Timeouts: {}", self.timeouts.len())?;
        writeln!(f, "🔀 Redirected: {}", self.redirects.len())?;
        writeln!(f, "👻 Excluded: {}", self.excludes.len())?;
        writeln!(f, "🚫 Errors: {}", self.errors.len() + self.failures.len())
    }
}
