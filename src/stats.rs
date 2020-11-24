use std::{
    collections::HashSet,
    fmt::{self, Display},
};

use crate::types::Response;
use crate::types::Uri;

pub struct ResponseStats {
    total: usize,
    successes: usize,
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
            successes: 0,
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
        match response.status {
            crate::types::Status::Ok(_) => self.successes += 1,
            crate::types::Status::Failed(_) => {
                self.failures.insert(uri);
            }
            crate::types::Status::Timeout => {
                self.timeouts.insert(uri);
            }
            crate::types::Status::Redirected => {
                self.redirects.insert(uri);
            }
            crate::types::Status::Excluded => {
                self.excludes.insert(uri);
            }
            crate::types::Status::Error(_) => {
                self.errors.insert(uri);
            }
        };
    }

    pub fn is_success(&self) -> bool {
        self.total == self.successes + self.excludes.len()
    }
}

impl Display for ResponseStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "📝 Summary")?;
        writeln!(f, "-------------------")?;
        writeln!(f, "🔍 Total: {}", self.total)?;
        writeln!(f, "✅ successes: {}", self.successes)?;
        writeln!(f, "⏳ timeouts: {}", self.timeouts.len())?;
        writeln!(f, "🔀 redirects: {}", self.redirects.len())?;
        writeln!(f, "👻 Excluded: {}", self.excludes.len())?;
        writeln!(f, "🚫 Errors: {}", self.errors.len() + self.failures.len())?;
        Ok(())
    }
}
